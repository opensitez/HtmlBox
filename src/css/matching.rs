//! Selector MATCHING: the ancestor Bloom filter that rejects most selectors
//! without a tree walk, the match context, and the matcher itself.
//!
//! ⛔ Named for the Bloom filter when it was split out, which described 50 of
//! its 770 lines. A file name that does not say what is in it is the same
//! problem as a `mod.rs` that holds everything.

#![allow(unused_imports)]
use std::collections::{HashMap, HashSet};
use rayon::prelude::*;
use crate::types::*;
use super::*;

// ─── Bloom filter for fast ancestor rejection ────────────────────────────────

/// A compact Bloom filter for ancestor tag/class names.
/// Used to quickly reject selectors that require an ancestor with a specific
/// tag or class that no ancestor has. False positives are OK (full match catches them),
/// false negatives are not (would incorrectly skip matching rules).
#[derive(Clone)]
pub struct AncestorBloom {
    bits: [u64; 4], // 256 bits
}

impl AncestorBloom {
    pub fn new() -> Self { Self { bits: [0; 4] } }

    #[inline]
    fn hash(s: &str) -> (usize, usize) {
        // Two simple hash functions for double-hashing
        let mut h1: u32 = 0x811c9dc5;
        let mut h2: u32 = 0;
        for &b in s.as_bytes() {
            h1 = h1.wrapping_mul(0x01000193) ^ (b as u32);
            h2 = h2.wrapping_add(b as u32).wrapping_mul(31);
        }
        ((h1 as usize) % 256, (h2 as usize) % 256)
    }

    #[inline]
    pub fn add(&mut self, s: &str) {
        let (h1, h2) = Self::hash(s);
        self.bits[h1 / 64] |= 1 << (h1 % 64);
        self.bits[h2 / 64] |= 1 << (h2 % 64);
    }

    /// Add a node's tag and class names to the filter.
    pub fn add_element(&mut self, tag: &str, attrs: &std::collections::HashMap<String, String>) {
        self.add(tag);
        if let Some(cls) = attrs.get("class") {
            for c in cls.split_whitespace() { self.add(c); }
        }
        if let Some(id) = attrs.get("id") { self.add(id); }
    }

    #[inline]
    pub fn might_contain(&self, s: &str) -> bool {
        let (h1, h2) = Self::hash(s);
        (self.bits[h1 / 64] & (1 << (h1 % 64))) != 0
            && (self.bits[h2 / 64] & (1 << (h2 % 64))) != 0
    }
}

impl Default for AncestorBloom { fn default() -> Self { Self::new() } }
impl std::fmt::Debug for AncestorBloom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AncestorBloom({} bits set)", self.bits.iter().map(|w| w.count_ones()).sum::<u32>())
    }
}

/// Info about one ancestor box, threaded through the cascade for selector matching.
#[derive(Clone, Debug, Default)]
pub struct AncestorInfo {
    pub tag:                String,
    pub attributes:         crate::dom::attrs::AttrMap,
    pub child_index:        usize,   // 0-based position among parent's children
    pub sibling_count:      usize,   // total children of parent
    pub type_child_index:   usize,   // 0-based among same-tag siblings
    pub type_sibling_count: usize,   // count of same-tag siblings
    pub node_id:            u32,  // stable node id for hover chain check
}

/// Extra context passed down through selector matching.
#[derive(Clone, Copy, Debug)]
pub struct MatchContext<'a> {
    /// Node ID of the focused element (0 = none).
    pub focused_box:        u32,
    /// True when focus was moved by keyboard (Tab/Shift+Tab) — drives :focus-visible.
    pub keyboard_focus:     bool,
    /// 0-based position among same-tag siblings.
    pub type_child_index:   usize,
    /// Count of same-tag siblings (including this element).
    pub type_sibling_count: usize,
    /// Raw pointer to the WebCore being matched (for :has()).
    pub html_box:           Option<&'a crate::types::WebCore>,
    /// Set of node IDs on the hover chain (hovered element + all ancestors).
    /// When non-empty, :hover pseudo-class matches elements in this set.
    pub hover_chain:        &'a std::collections::HashSet<u32>,
    /// Node ID of the element currently being matched (for :hover on ancestors).
    pub element_id:         u32,
    /// Previous non-text sibling info for `+` and `~` combinators.
    /// Each entry: (tag, id, classes) of preceding element siblings.
    pub prev_siblings:      &'a [(String, String, String)],
}


/// Recursively match a selector (parts slice) against a subject element + its ancestor chain.
/// Works right-to-left: the last segment matches the subject, preceding segments
/// must match ancestors according to the combinator between them.
pub fn matches_selector_with_ancestors(
    parts: &[SelectorPart],
    tag: &str,
    attrs: &crate::dom::attrs::AttrMap,
    child_index: usize,
    sibling_count: usize,
    ancestors: &[AncestorInfo],
    ctx: &MatchContext<'_>,
) -> bool {
    if parts.is_empty() { return true; }

    // Find the rightmost combinator in `parts`
    let last_comb_pos = parts.iter().rposition(|p| matches!(p, SelectorPart::Combinator(_)));

    match last_comb_pos {
        None => {
            // No combinator — all parts must match the subject
            parts.iter().all(|p| matches_part_with_context(p, tag, attrs, child_index, sibling_count, ancestors, ctx))
        }
        Some(pos) => {
            let combinator = match &parts[pos] {
                SelectorPart::Combinator(c) => c.clone(),
                _ => unreachable!(),
            };
            let left_parts  = &parts[..pos];
            let right_parts = &parts[pos + 1..];

            // Right parts must all match the subject
            if !right_parts.iter().all(|p| matches_part_with_context(p, tag, attrs, child_index, sibling_count, ancestors, ctx)) {
                return false;
            }

            match combinator {
                Combinator::Descendant => {
                    // Left parts must match any ancestor
                    for (i, anc) in ancestors.iter().enumerate() {
                        let anc_ctx = MatchContext {
                            focused_box: ctx.focused_box,
                            keyboard_focus: ctx.keyboard_focus,
                            type_child_index: anc.type_child_index,
                            type_sibling_count: anc.type_sibling_count,
                            html_box: None,
                            hover_chain: ctx.hover_chain,
                            element_id: anc.node_id,
            prev_siblings: &[],
                        };
                        if matches_selector_with_ancestors(
                            left_parts,
                            &anc.tag, &anc.attributes,
                            anc.child_index, anc.sibling_count,
                            &ancestors[..i],
                            &anc_ctx,
                        ) {
                            return true;
                        }
                    }
                    false
                }
                Combinator::Child => {
                    // Left parts must match the direct parent (last ancestor)
                    if let Some(parent) = ancestors.last() {
                        let parent_ancestors = &ancestors[..ancestors.len() - 1];
                        let parent_ctx = MatchContext {
                            focused_box: ctx.focused_box,
                            keyboard_focus: ctx.keyboard_focus,
                            type_child_index: parent.type_child_index,
                            type_sibling_count: parent.type_sibling_count,
                            html_box: None,
                            hover_chain: ctx.hover_chain,
                            element_id: parent.node_id,
            prev_siblings: &[],
                        };
                        matches_selector_with_ancestors(
                            left_parts,
                            &parent.tag, &parent.attributes,
                            parent.child_index, parent.sibling_count,
                            parent_ancestors,
                            &parent_ctx,
                        )
                    } else {
                        false
                    }
                }
                Combinator::AdjacentSibling => {
                    // Match left_parts against the immediately previous non-text sibling
                    if let Some(last) = ctx.prev_siblings.last() {
                        let mut sib_attrs = crate::dom::attrs::AttrMap::new();
                        if !last.1.is_empty() { sib_attrs.insert("id".to_string(), last.1.clone()); }
                        if !last.2.is_empty() { sib_attrs.insert("class".to_string(), last.2.clone()); }
                        left_parts.iter().all(|p| matches_part_with_context(
                            p, &last.0, &sib_attrs, 0, 0, ancestors, ctx,
                        ))
                    } else {
                        false
                    }
                }
                Combinator::GeneralSibling => {
                    // Match left_parts against ANY previous non-text sibling
                    ctx.prev_siblings.iter().any(|sib| {
                        let mut sib_attrs = crate::dom::attrs::AttrMap::new();
                        if !sib.1.is_empty() { sib_attrs.insert("id".to_string(), sib.1.clone()); }
                        if !sib.2.is_empty() { sib_attrs.insert("class".to_string(), sib.2.clone()); }
                        left_parts.iter().all(|p| matches_part_with_context(
                            p, &sib.0, &sib_attrs, 0, 0, ancestors, ctx,
                        ))
                    })
                }
            }
        }
    }
}

pub(crate) fn matches_part_with_context(
    part: &SelectorPart,
    tag: &str,
    attrs: &crate::dom::attrs::AttrMap,
    child_index: usize,
    sibling_count: usize,
    ancestors: &[AncestorInfo],
    ctx: &MatchContext<'_>,
) -> bool {
    match part {
        SelectorPart::Universal => true,
        SelectorPart::Tag(t)    => tag.eq_ignore_ascii_case(t),
        SelectorPart::Id(id)    => attrs.get("id").map(|s| s == id).unwrap_or(false),
        SelectorPart::Class(cls) => attrs.get("class")
            .map(|s| s.split_whitespace().any(|c| c == cls))
            .unwrap_or(false),
        SelectorPart::Attribute { name, op, value, case_sensitive } => {
            // **An attribute NAME in a selector is ASCII case-insensitive for
            // an HTML document.** HTML folds attribute names on the way in, so
            // `[DATA-Foo]` and `[data-foo]` name the same attribute — and a
            // page that writes `setAttribute("DATA-Foo", …)` and then styles
            // `[DATA-Foo]` is one a browser handles without comment.
            //
            // Exact FIRST, fold only on a miss — the same shape the namespace
            // tree settled on, and it leaves an XML document (whose attribute
            // names keep their case) matching exactly as it must.
            //
            // The tag name already worked this way (`eq_ignore_ascii_case`);
            // the attribute name was the half nobody folded, so `[DATA-foo]`
            // silently matched nothing.
            let found = attrs.get(name).or_else(|| {
                attrs
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
                    .map(|(_, v)| v)
            });
            // Selectors §6.3. An explicit `s` forces case-sensitive matching,
            // an explicit `i` forces case-insensitive; with no flag the value
            // is matched case-sensitively, which is what HTML asks for on every
            // attribute the UA sheet does not tag with `i`.
            let fold = case_sensitive == &Some(false);
            let av_owned;
            let val_owned;
            let (av_cmp, val_cmp): (&str, &str) = match found {
                None => return false,
                Some(av) if fold => {
                    av_owned  = av.to_ascii_lowercase();
                    val_owned = value.to_ascii_lowercase();
                    (&av_owned, &val_owned)
                }
                Some(av) => (av.as_str(), value.as_str()),
            };
            match op {
                AttrOp::Exists     => true,
                AttrOp::Eq         => av_cmp == val_cmp,
                AttrOp::Includes   => av_cmp.split_whitespace().any(|w| w == val_cmp),
                AttrOp::StartsWith => av_cmp.starts_with(val_cmp),
                AttrOp::EndsWith   => av_cmp.ends_with(val_cmp),
                AttrOp::Contains   => av_cmp.contains(val_cmp),
                AttrOp::DashMatch  => av_cmp == val_cmp || av_cmp.starts_with(&format!("{}-", val_cmp)),
            }
        }
        SelectorPart::Not(inner) => {
            !inner.matches_with_ancestors_ctx_raw(tag, attrs, child_index, sibling_count, ancestors, ctx)
        }
        SelectorPart::Is(list) => {
            list.iter().any(|sel| sel.matches_with_ancestors_ctx_raw(tag, attrs, child_index, sibling_count, ancestors, ctx))
        }
        SelectorPart::Where(list) => {
            list.iter().any(|sel| sel.matches_with_ancestors_ctx_raw(tag, attrs, child_index, sibling_count, ancestors, ctx))
        }
        SelectorPart::Has(inner) => {
            // Check if any descendant of the current element matches inner
            if let Some(b) = ctx.html_box {
                has_descendant_matching(b, inner, ctx.focused_box)
            } else {
                false
            }
        }
        SelectorPart::PseudoClass(pc) => {
            let pc = pc.as_str();
            match pc {
                "first-child"  => child_index == 0,
                "last-child"   => child_index + 1 == sibling_count,
                "only-child"   => sibling_count == 1,
                "first-of-type" => ctx.type_child_index == 0,
                "last-of-type"  => ctx.type_child_index + 1 == ctx.type_sibling_count,
                "only-of-type"  => ctx.type_sibling_count == 1,
                "root"         => tag.eq_ignore_ascii_case("html"),
                // Selectors §14.3 — no element children and no TEXT children.
                // Comments and processing instructions do not count, which is
                // why this asks `is_element`/`is_text_node` rather than
                // `children.is_empty()`: since comments became real nodes,
                // `<div><!--x--></div>` has a child and is still `:empty`.
                //
                // `html_box` is `None` when the subject is an ANCESTOR being
                // matched for a combinator, and false is the right answer
                // there: an `:empty` element has no descendants for the rest of
                // the selector to have matched.
                "empty" => match ctx.html_box {
                    Some(b) => !b.children.iter().any(|c|
                        c.is_element() || (c.is_text_node() && !c.text.is_empty())),
                    None => false,
                },
                // Focus
                "focus" => {
                    if ctx.focused_box != 0 {
                        if let Some(b) = ctx.html_box {
                            b.node_id != 0 && b.node_id == ctx.focused_box
                        } else { false }
                    } else { false }
                }
                // :focus-visible matches when focus arrived via keyboard, OR when the
                // element is a text-entry control (input, textarea, contenteditable) —
                // matching browser behaviour where the caret always needs a visible ring.
                "focus-visible" => {
                    if ctx.focused_box == 0 { return false; }
                    if let Some(b) = ctx.html_box {
                        if b.node_id == 0 || b.node_id != ctx.focused_box {
                            return false;
                        }
                        ctx.keyboard_focus || is_text_entry(b)
                    } else {
                        // Fallback: use element_id when html_box not available
                        ctx.element_id != 0 && ctx.element_id == ctx.focused_box && ctx.keyboard_focus
                    }
                }
                "focus-within" => {
                    if ctx.focused_box != 0 {
                        if let Some(b) = ctx.html_box {
                            // Is this box itself focused, or does it contain the focused element?
                            (b.node_id != 0 && b.node_id == ctx.focused_box)
                                || is_or_contains_focused(b, ctx.focused_box)
                        } else { false }
                    } else { false }
                }
                // Form state
                // `:checked` matches CHECKEDNESS (Selectors §11.1: "elements
                // that are checked"), not the `checked` attribute. Matching the
                // attribute meant a box the user ticked kept the unchecked
                // styling while the painter drew a tick — the selector and the
                // paint disagreeing about one box.
                // Top-layer membership (CSS Position §6). Both of these were
                // in `is_known_pseudo_class` — so they PARSED — with no arm
                // here at all: recognised names that matched nothing, which is
                // indistinguishable from a rule that never applies.
                "modal" => matches!(
                    ctx.html_box.and_then(|b| b.top_layer_kind),
                    Some(crate::types::TopLayerKind::ModalDialog)
                ),
                "popover-open" => matches!(
                    ctx.html_box.and_then(|b| b.top_layer_kind),
                    Some(crate::types::TopLayerKind::Popover)
                ),
                "checked"    => {
                    // The BOX's state when the matcher was given one; the
                    // attribute is the fallback for the paths that match
                    // against a bare tag+attrs (an `<option selected>` has no
                    // checkedness of its own).
                    ctx.html_box.map(|b| b.checkedness).unwrap_or_else(|| attrs.contains_key("checked"))
                        || attrs.contains_key("selected")
                }
                // Disabledness is INHERITED from a disabled `<fieldset>`
                // (HTML §4.10.19.6), so the attribute on the element itself is
                // only half the question. The ancestor chain carries each
                // ancestor's attributes, which is what makes the second half
                // answerable on the raw tag+attrs path as well as the box path.
                "disabled"   => is_actually_disabled(tag, attrs, ancestors),
                "enabled"    => is_form_control(tag) && !is_actually_disabled(tag, attrs, ancestors),
                "read-only"  => attrs.contains_key("readonly") || !matches!(tag, "input" | "textarea" | "select" | "button"),
                "read-write" => !attrs.contains_key("readonly") && matches!(tag, "input" | "textarea" | "select" | "button"),
                // Link
                "any-link" | "link" => {
                    attrs.contains_key("href") && matches!(tag, "a" | "area" | "link")
                }
                "visited" | "active" => false,
                "hover" => {
                    // :hover matches when the element being matched is in the
                    // hover chain (the hovered element + all its ancestors).
                    if !ctx.hover_chain.is_empty() && ctx.element_id != 0 {
                        ctx.hover_chain.contains(&ctx.element_id)
                    } else {
                        false
                    }
                }
                // HTML §4.16.3. `required`/`optional` are a partition over the
                // controls that CAN be required — a `<div required>` is
                // neither, and neither is an `<input type=button required>`.
                "required" => is_requirable(tag, attrs) && attrs.contains_key("required"),
                "optional" => is_requirable(tag, attrs) && !attrs.contains_key("required"),
                // The placeholder is showing when there IS one and the control
                // is empty. The box carries the live value; `value` is the
                // fallback for the paths that match on bare tag+attrs.
                "placeholder-shown" => {
                    let has_placeholder = attrs.get("placeholder")
                        .map(|p| !p.is_empty()).unwrap_or(false);
                    if !has_placeholder || !matches!(tag, "input" | "textarea") { return false; }
                    match ctx.html_box.and_then(|b| b.value_state.as_ref()) {
                        Some(v) => v.is_empty(),
                        None    => attrs.get("value").map(|v| v.is_empty()).unwrap_or(true),
                    }
                }
                // A checkbox or radio put in the indeterminate state by script.
                // It is NOT the `indeterminate` content attribute — there isn't
                // one — so a box is the only place the answer can come from.
                "indeterminate" => ctx.html_box
                    .map(|b| b.data.get("indeterminate").map(|v| v == "true").unwrap_or(false))
                    .unwrap_or(false),
                // `:default` covers two things, and only one of them is
                // answerable here. A checkbox, radio or option that was checked
                // IN THE MARKUP is `:default` — the attribute, deliberately,
                // not the current checkedness, so unticking a box does not stop
                // it being the default.
                //
                // The other half is the form's DEFAULT BUTTON: the first submit
                // button in tree order whose form owner is this form. That
                // needs to compare an element against its siblings through a
                // form owner, and the matcher is given one element plus its
                // ancestors — so answering it here would mean "every submit
                // button is the default", which is a wrong answer rather than a
                // missing one. It is left out until the match context can carry
                // the form.
                "default" => match tag {
                    "input"  => attrs.contains_key("checked"),
                    "option" => attrs.contains_key("selected"),
                    _ => false,
                },
                // Constraint validation is not implemented, so every one of
                // these answers "no". They are RECOGNISED (see
                // `is_known_pseudo_class`) so that `input:invalid { … }` keeps
                // its rule instead of having it dropped as a syntax error —
                // a missing feature, not an invalid selector.
                "valid" | "invalid" | "in-range" | "out-of-range"
                | "user-valid" | "user-invalid" | "blank" | "autofill" => false,
                _ => {
                    // nth-child(expr) / nth-of-type(expr)
                    if let Some(inner) = pc.strip_prefix("nth-child(").and_then(|s| s.strip_suffix(')')) {
                        return nth_matches(inner, child_index + 1); // CSS is 1-based
                    }
                    if let Some(inner) = pc.strip_prefix("nth-last-child(").and_then(|s| s.strip_suffix(')')) {
                        let from_end = sibling_count - child_index; // 1-based from end
                        return nth_matches(inner, from_end);
                    }
                    if let Some(inner) = pc.strip_prefix("nth-of-type(").and_then(|s| s.strip_suffix(')')) {
                        return nth_matches(inner, ctx.type_child_index + 1);
                    }
                    if let Some(inner) = pc.strip_prefix("nth-last-of-type(").and_then(|s| s.strip_suffix(')')) {
                        let from_end = ctx.type_sibling_count - ctx.type_child_index;
                        return nth_matches(inner, from_end);
                    }
                    // Shadow DOM pseudo-classes: never match in non-shadow context
                    if pc.starts_with("host(") || pc.starts_with("host-context(") || pc == "host" {
                        return false;
                    }
                    // Everything left is a pseudo-class this engine recognises
                    // but has no state for — `:target` with no URL fragment,
                    // `:modal`, `:fullscreen`, `:lang()`, the media-resource
                    // ones. They do not match.
                    //
                    // This used to `return true` "for forward compat", which
                    // meant `:target` matched EVERY element and any typo styled
                    // the whole document. Selectors are validated at parse time
                    // now (`is_known_pseudo_class`), so an unrecognised name has
                    // already taken its rule down before reaching the matcher
                    // and there is nothing left here to be lenient about.
                    false
                }
            }
        }
        SelectorPart::PseudoElement(_) => false, // pseudo-elements never match real elements
        SelectorPart::Combinator(_)    => true,
    }
}

/// The elements `:enabled` / `:disabled` are defined over (HTML §4.16.3):
/// form controls, plus `<fieldset>`, `<optgroup>` and `<option>`.
fn is_form_control(tag: &str) -> bool {
    matches!(tag, "input" | "button" | "select" | "textarea"
                | "fieldset" | "optgroup" | "option")
}

/// Is this element disabled, counting a disabled `<fieldset>` ancestor?
///
/// HTML §4.10.19.6: a control inside a disabled `<fieldset>` is itself
/// disabled, unless it sits in that fieldset's FIRST `<legend>`. The legend
/// exemption needs to know which legend is first among its siblings, which the
/// ancestor chain does record — `child_index` on the legend's own entry.
fn is_actually_disabled(tag: &str, attrs: &crate::dom::attrs::AttrMap, ancestors: &[AncestorInfo]) -> bool {
    if !is_form_control(tag) { return false; }
    if attrs.contains_key("disabled") { return true; }
    // Walk from the element outwards. A `<legend>` seen on the way up shields
    // the element from the fieldset that legend belongs to — but only if it is
    // that fieldset's first legend, and only for the fieldset immediately
    // outside it.
    let mut shielded_by_legend = false;
    for anc in ancestors.iter().rev() {
        if anc.tag == "legend" {
            // The FIRST `<legend>` child, not the first child: in
            // `<fieldset disabled><p>x</p><legend><input></legend>` the legend
            // is child 1 and still shields. `type_child_index` counts among
            // same-tag siblings, which is exactly that question.
            shielded_by_legend = anc.type_child_index == 0;
            continue;
        }
        if anc.tag == "fieldset" && anc.attributes.contains_key("disabled") && !shielded_by_legend {
            return true;
        }
        shielded_by_legend = false;
    }
    false
}

/// Can this element be `required`? Only the controls that take the attribute —
/// `:optional` is "requirable but not required", so a `<div>` is neither.
fn is_requirable(tag: &str, attrs: &crate::dom::attrs::AttrMap) -> bool {
    match tag {
        "select" | "textarea" => true,
        "input" => !matches!(
            attrs.get("type").map(|s| s.to_ascii_lowercase()).as_deref(),
            Some("hidden") | Some("range") | Some("color") | Some("submit")
            | Some("image") | Some("reset") | Some("button")
        ),
        _ => false,
    }
}

/// Returns true for text-entry controls — these always show :focus-visible even
/// when focused by mouse, because the cursor position needs to be visible.
fn is_text_entry(b: &crate::types::WebCore) -> bool {
    match b.tag.as_str() {
        "textarea" => true,
        "input" => !matches!(
            b.attributes.get("type").map(|s| s.as_str()),
            Some("button" | "submit" | "reset" | "checkbox" | "radio" | "range" | "color" | "hidden")
        ),
        _ => b.attributes.get("contenteditable")
                .map(|v| v == "true" || v.is_empty())
                .unwrap_or(false),
    }
}

/// Check if `b` or any of its descendants is the focused element.
fn is_or_contains_focused(b: &crate::types::WebCore, focused: u32) -> bool {
    for child in &b.children {
        if child.node_id != 0 && child.node_id == focused {
            return true;
        }
        if is_or_contains_focused(child, focused) {
            return true;
        }
    }
    false
}

/// Check if any descendant of `node` matches `sel`.
fn has_descendant_matching(
    node: &crate::types::WebCore,
    sel: &CssSelector,
    focused_box: u32,
) -> bool {
    let empty_hover = std::collections::HashSet::new();
    for child in &node.children {
        let ctx = MatchContext {
            focused_box,
            keyboard_focus: false,
            type_child_index: 0,
            type_sibling_count: 1,
            html_box: Some(child),
            hover_chain: &empty_hover,
            element_id: child.node_id,
            prev_siblings: &[],
        };
        if matches_selector_with_ancestors(&sel.parts, &child.tag, &child.attributes, 0, 1, &[], &ctx) {
            return true;
        }
        if has_descendant_matching(child, sel, focused_box) {
            return true;
        }
    }
    false
}

/// Evaluate CSS An+B formula against a 1-based position.
fn nth_matches(expr: &str, pos: usize) -> bool {
    let expr = expr.trim();
    match expr {
        "odd"  => pos % 2 == 1,
        "even" => pos % 2 == 0,
        _ => {
            if let Ok(n) = expr.parse::<i32>() {
                return pos as i32 == n;
            }
            let (a, b) = parse_nth_ab(expr);
            if a == 0 {
                return pos as i32 == b;
            }
            let diff = pos as i32 - b;
            if a > 0 { diff >= 0 && diff % a == 0 }
            else     { diff <= 0 && diff % a == 0 }
        }
    }
}

fn parse_nth_ab(expr: &str) -> (i32, i32) {
    if let Some(n_pos) = expr.find('n') {
        let a_str = expr[..n_pos].trim();
        let b_str = expr[n_pos + 1..].trim();
        let a: i32 = match a_str {
            "" | "+" => 1,
            "-"      => -1,
            s        => s.parse().unwrap_or(0),
        };
        let b: i32 = if b_str.is_empty() { 0 } else { b_str.parse().unwrap_or(0) };
        (a, b)
    } else {
        (0, expr.parse().unwrap_or(0))
    }
}
