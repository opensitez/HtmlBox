pub mod properties;
pub mod property_defs;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use rayon::prelude::*;

/// Global counter for allocating unique node_ids for pseudo-elements and
/// dynamically created nodes during cascade (where Document isn't accessible).
/// Starts at a high value to avoid colliding with parser-assigned IDs.
static PSEUDO_NODE_ID: AtomicU32 = AtomicU32::new(1_000_000);

fn alloc_pseudo_node_id() -> u32 {
    PSEUDO_NODE_ID.fetch_add(1, Ordering::Relaxed)
}
use crate::types::*;

// ─── CSS Rule & Selector ─────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum SelectorPart {
    Tag(String),
    Id(String),
    Class(String),
    Universal,
    PseudoClass(String),
    PseudoElement(String),
    Attribute { name: String, op: AttrOp, value: String },
    Combinator(Combinator),
    /// :not(selector)
    Not(Box<CssSelector>),
    /// :is(selector-list)
    Is(Vec<CssSelector>),
    /// :where(selector-list)  — same as Is but zero specificity
    Where(Vec<CssSelector>),
    /// :has(selector) — matches if any descendant matches
    Has(Box<CssSelector>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum AttrOp { Exists, Eq, Contains, StartsWith, EndsWith, Includes, DashMatch }

#[derive(Clone, Debug, PartialEq)]
pub enum Combinator { Descendant, Child, AdjacentSibling, GeneralSibling }

#[derive(Clone, Debug, PartialEq)]
pub struct CssSelector {
    pub parts: Vec<SelectorPart>,
    /// Pre-computed state pseudo-class flags (set during parse, avoids per-match scan).
    pub has_hover:   bool,
    pub has_active:  bool,
    pub has_visited: bool,
}

/// Info about one ancestor box, threaded through the cascade for selector matching.
#[derive(Clone, Debug, Default)]
pub struct AncestorInfo {
    pub tag:                String,
    pub attributes:         HashMap<String, String>,
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
    /// Raw pointer to the HtmlBox being matched (for :has()).
    pub html_box:           Option<&'a crate::types::HtmlBox>,
    /// Set of node IDs on the hover chain (hovered element + all ancestors).
    /// When non-empty, :hover pseudo-class matches elements in this set.
    pub hover_chain:        &'a std::collections::HashSet<u32>,
    /// Node ID of the element currently being matched (for :hover on ancestors).
    pub element_id:         u32,
}

impl CssSelector {
    /// Create a selector with pre-computed state pseudo-class flags.
    pub fn new(parts: Vec<SelectorPart>) -> Self {
        let has_hover = parts.iter().any(|p| matches!(p, SelectorPart::PseudoClass(n) if n == "hover"));
        let has_active = parts.iter().any(|p| matches!(p, SelectorPart::PseudoClass(n) if n == "active"));
        let has_visited = parts.iter().any(|p| matches!(p, SelectorPart::PseudoClass(n) if n == "visited"));
        Self { parts, has_hover, has_active, has_visited }
    }

    pub fn specificity(&self) -> u32 {
        let mut ids = 0u32;
        let mut classes = 0u32;
        let mut elements = 0u32;
        for part in &self.parts {
            match part {
                SelectorPart::Id(_)             => ids     += 1,
                SelectorPart::Class(_)
                | SelectorPart::PseudoClass(_)
                | SelectorPart::Attribute { .. } => classes += 1,
                SelectorPart::Tag(t) if t != "*" => elements += 1,
                SelectorPart::PseudoElement(_)   => elements += 1,
                // :not() contributes the inner selector's specificity
                SelectorPart::Not(inner)         => {
                    let s = inner.specificity();
                    ids     += s / 100;
                    classes += (s % 100) / 10;
                    elements+= s % 10;
                }
                // :is() contributes the most-specific inner selector
                SelectorPart::Is(list)           => {
                    let max_sp = list.iter().map(|s| s.specificity()).max().unwrap_or(0);
                    ids     += max_sp / 100;
                    classes += (max_sp % 100) / 10;
                    elements+= max_sp % 10;
                }
                // :where() contributes zero specificity
                SelectorPart::Where(_)           => {}
                // :has() contributes the inner selector's specificity
                SelectorPart::Has(inner)         => {
                    let s = inner.specificity();
                    ids     += s / 100;
                    classes += (s % 100) / 10;
                    elements+= s % 10;
                }
                _ => {}
            }
        }
        ids * 100 + classes * 10 + elements
    }

    /// Match against `b` without ancestor context (for tests / simple selectors).
    pub fn matches_box(&self, b: &HtmlBox) -> bool {
        let empty_hover = std::collections::HashSet::new();
        let ctx = MatchContext {
            focused_box: 0,
            keyboard_focus: false,
            type_child_index: 0,
            type_sibling_count: 1,
            html_box: Some(b),
            hover_chain: &empty_hover,
            element_id: b.node_id,
        };
        matches_selector_with_ancestors(&self.parts, &b.tag, &b.attributes, 0, 1, &[], &ctx)
    }

    /// Match against `b` with full ancestor chain for combinator resolution.
    pub fn matches_with_ancestors(
        &self,
        b: &HtmlBox,
        child_index: usize,
        sibling_count: usize,
        ancestors: &[AncestorInfo],
    ) -> bool {
        let empty_hover = std::collections::HashSet::new();
        let ctx = MatchContext {
            focused_box: 0,
            keyboard_focus: false,
            type_child_index: 0,
            type_sibling_count: 1,
            html_box: Some(b),
            hover_chain: &empty_hover,
            element_id: b.node_id,
        };
        matches_selector_with_ancestors(&self.parts, &b.tag, &b.attributes, child_index, sibling_count, ancestors, &ctx)
    }

    /// Match against `b` with full ancestor chain and extra context.
    pub fn matches_with_ancestors_ctx(
        &self,
        b: &HtmlBox,
        child_index: usize,
        sibling_count: usize,
        ancestors: &[AncestorInfo],
        ctx: &MatchContext<'_>,
    ) -> bool {
        matches_selector_with_ancestors(&self.parts, &b.tag, &b.attributes, child_index, sibling_count, ancestors, ctx)
    }

    /// Internal: match using raw tag/attrs (used from :not/:is/:where to avoid re-borrowing HtmlBox).
    fn matches_with_ancestors_ctx_raw(
        &self,
        tag: &str,
        attrs: &HashMap<String, String>,
        child_index: usize,
        sibling_count: usize,
        ancestors: &[AncestorInfo],
        ctx: &MatchContext<'_>,
    ) -> bool {
        matches_selector_with_ancestors(&self.parts, tag, attrs, child_index, sibling_count, ancestors, ctx)
    }
}

/// Recursively match a selector (parts slice) against a subject element + its ancestor chain.
/// Works right-to-left: the last segment matches the subject, preceding segments
/// must match ancestors according to the combinator between them.
pub fn matches_selector_with_ancestors(
    parts: &[SelectorPart],
    tag: &str,
    attrs: &HashMap<String, String>,
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
                Combinator::AdjacentSibling | Combinator::GeneralSibling => {
                    // We don't have sibling element data in the ancestor chain,
                    // so we can't fully resolve these — skip for now.
                    false
                }
            }
        }
    }
}

fn matches_part_with_context(
    part: &SelectorPart,
    tag: &str,
    attrs: &HashMap<String, String>,
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
        SelectorPart::Attribute { name, op, value } => {
            match attrs.get(name) {
                None     => false,
                Some(av) => match op {
                    AttrOp::Exists     => true,
                    AttrOp::Eq         => av == value,
                    AttrOp::Includes   => av.split_whitespace().any(|w| w == value),
                    AttrOp::StartsWith => av.starts_with(value.as_str()),
                    AttrOp::EndsWith   => av.ends_with(value.as_str()),
                    AttrOp::Contains   => av.contains(value.as_str()),
                    AttrOp::DashMatch  => av == value || av.starts_with(&format!("{}-", value)),
                }
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
                "empty"        => false, // can't tell from style alone
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
                    } else { false }
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
                "checked"    => attrs.contains_key("checked") || attrs.contains_key("selected"),
                "disabled"   => attrs.contains_key("disabled"),
                "enabled"    => !attrs.contains_key("disabled") && matches!(tag, "input" | "button" | "select" | "textarea"),
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
                "placeholder-shown" | "required" | "optional" | "valid" | "invalid" => false,
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
                    // Unknown pseudo-class: fail-open for forward compat
                    true
                }
            }
        }
        SelectorPart::PseudoElement(_) => false, // pseudo-elements never match real elements
        SelectorPart::Combinator(_)    => true,
    }
}

/// Returns true for text-entry controls — these always show :focus-visible even
/// when focused by mouse, because the cursor position needs to be visible.
fn is_text_entry(b: &crate::types::HtmlBox) -> bool {
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
fn is_or_contains_focused(b: &crate::types::HtmlBox, focused: u32) -> bool {
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
    node: &crate::types::HtmlBox,
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

// ─── CSS Rule ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum PseudoElement {
    None,         // regular rule
    Before,       // ::before
    After,        // ::after
    Selection,    // ::selection
    Marker,       // ::marker
    Ignored,      // ::first-line, ::first-letter, ::placeholder, unknown vendor pseudo-elements
}

impl Default for PseudoElement {
    fn default() -> Self { Self::None }
}

#[derive(Clone, Debug)]
pub struct CssRule {
    pub selectors:           Vec<CssSelector>,
    pub declarations:        HashMap<String, String>,
    pub important_declarations: HashMap<String, String>,
    /// Pre-resolved declarations: (PropertyId, value_string).
    /// Populated during `compile_declarations()`. Used by the cascade for
    /// fast enum dispatch instead of string matching.
    pub compiled_decls:      Vec<(properties::PropertyId, String)>,
    /// Pre-resolved important declarations.
    pub compiled_important:  Vec<(properties::PropertyId, String)>,
    pub specificity:         u32,     // max of all selectors
    pub media_condition:     String,  // non-empty if inside @media
    pub container_condition: String,  // non-empty if inside @container
    pub container_name:      String,  // optional container name (empty = unnamed)
    pub original_selector:   String,  // verbatim selector text for roundtrip
    pub is_hover:            bool,
    /// True if any declaration value contains `var(` — needs slow-path resolution.
    pub has_var_refs:        bool,
    pub pseudo_element:      PseudoElement,
}

impl Default for CssRule {
    fn default() -> Self {
        Self {
            selectors:           Vec::new(),
            declarations:        HashMap::new(),
            important_declarations: HashMap::new(),
            compiled_decls:      Vec::new(),
            compiled_important:  Vec::new(),
            specificity:         0,
            media_condition:     String::new(),
            container_condition: String::new(),
            container_name:      String::new(),
            original_selector:   String::new(),
            is_hover:            false,
            has_var_refs:        false,
            pseudo_element:      PseudoElement::None,
        }
    }
}

impl CssRule {
    /// Pre-compile declarations from HashMap<String,String> into Vec<(PropertyId, String)>.
    /// Called during rebuild_index(). Shorthands are expanded into longhands.
    pub fn compile_declarations(&mut self) {
        self.compiled_decls.clear();
        for (prop, val) in &self.declarations {
            let id = properties::resolve(prop);
            if id == properties::PropertyId::Unknown { continue; }
            self.compiled_decls.push((id, val.clone()));
        }
        self.compiled_important.clear();
        for (prop, val) in &self.important_declarations {
            let id = properties::resolve(prop);
            if id == properties::PropertyId::Unknown { continue; }
            self.compiled_important.push((id, val.clone()));
        }
        // Pre-compute whether any declaration references var()
        self.has_var_refs = self.declarations.values().any(|v| v.contains("var("))
            || self.important_declarations.values().any(|v| v.contains("var("));
    }
}

// ─── Font utility helpers ──────────────────────────────────────────────────────

/// Split a CSS `font-family` value into individual family names.
/// Handles quoted names with spaces and strips surrounding quote characters.
/// `"Times New Roman", Arial, sans-serif` → `["Times New Roman", "Arial", "sans-serif"]`
pub fn split_font_families(raw: &str) -> Vec<String> {
    let mut families = Vec::new();
    let mut current  = String::new();
    let mut in_quotes = false;
    let mut quote_char = '"';

    for ch in raw.chars() {
        match ch {
            '"' | '\'' if !in_quotes => { in_quotes = true; quote_char = ch; }
            c if in_quotes && c == quote_char => { in_quotes = false; }
            ',' if !in_quotes => {
                let name = current.trim().to_string();
                if !name.is_empty() { families.push(name); }
                current.clear();
            }
            c => { current.push(c); }
        }
    }
    let name = current.trim().to_string();
    if !name.is_empty() { families.push(name); }
    families
}

/// Map CSS system-font keywords to a generic CSS family name.
/// Returns `None` for regular named fonts that should be kept as-is.
pub fn resolve_system_font_keyword(name: &str) -> Option<&'static str> {
    match name {
        "system-ui" | "-apple-system" | "BlinkMacSystemFont"
        | "ui-sans-serif" | "ui-rounded" => Some("sans-serif"),
        "ui-serif"     => Some("serif"),
        "ui-monospace" => Some("monospace"),
        _              => None,
    }
}

/// Parse a CSS `font-variation-settings` value into a list of `(axis-tag, value)` pairs.
/// Accepts `normal` (returns empty) or `"wght" 700, "wdth" 75`.
pub fn parse_variation_settings(v: &str) -> Vec<(String, f32)> {
    let v = v.trim();
    if v == "normal" { return Vec::new(); }
    let mut result = Vec::new();
    // Each entry: `"tag" value`, comma-separated.
    for entry in v.split(',') {
        let entry = entry.trim();
        // Find the quoted tag
        let (tag, rest) = if entry.starts_with('"') {
            let end = entry[1..].find('"').map(|i| i + 1);
            if let Some(end) = end {
                (&entry[1..end], entry[end + 1..].trim())
            } else { continue; }
        } else if entry.starts_with('\'') {
            let end = entry[1..].find('\'').map(|i| i + 1);
            if let Some(end) = end {
                (&entry[1..end], entry[end + 1..].trim())
            } else { continue; }
        } else { continue; };

        if let Ok(val) = rest.parse::<f32>() {
            result.push((tag.to_string(), val));
        }
    }
    result
}

/// Parse a CSS `font-feature-settings` value into `(feature-tag, value)` pairs.
/// Accepts `normal` (empty), `"kern"` (= 1), `"liga" on`, `"liga" off`, `"calt" 2`.
pub fn parse_feature_settings(v: &str) -> Vec<(String, u32)> {
    let v = v.trim();
    if v == "normal" { return Vec::new(); }
    let mut result = Vec::new();
    for entry in v.split(',') {
        let entry = entry.trim();
        let (tag, rest) = if entry.starts_with('"') {
            let end = entry[1..].find('"').map(|i| i + 1);
            if let Some(end) = end {
                (&entry[1..end], entry[end + 1..].trim())
            } else { continue; }
        } else if entry.starts_with('\'') {
            let end = entry[1..].find('\'').map(|i| i + 1);
            if let Some(end) = end {
                (&entry[1..end], entry[end + 1..].trim())
            } else { continue; }
        } else { continue; };

        let val = match rest {
            "" | "on"  => 1,
            "off"      => 0,
            s          => s.parse::<u32>().unwrap_or(1),
        };
        result.push((tag.to_string(), val));
    }
    result
}

// ─── @font-face declaration ───────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct FontFaceDecl {
    pub family: String,
    pub src:    String,
    pub weight: Option<String>,
    pub style:  Option<String>,
}

/// Extract a file path from a CSS url("...") or local("...") value.
pub fn extract_url_path(src: &str) -> String {
    let src = src.trim();
    // Strip url("...") or url('...')
    let inner = if let Some(s) = src.strip_prefix("url(") {
        s.trim_end_matches(')').trim().trim_matches('"').trim_matches('\'')
    } else if let Some(s) = src.strip_prefix("local(") {
        s.trim_end_matches(')').trim().trim_matches('"').trim_matches('\'')
    } else {
        src.trim_matches('"').trim_matches('\'')
    };
    inner.to_string()
}

// ─── Stylesheet ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct Stylesheet {
    pub rules:      Vec<CssRule>,
    pub variables:  HashMap<String, String>,  // CSS custom properties from :root
    pub font_faces: Vec<FontFaceDecl>,
    /// Parsed `@keyframes` blocks, keyed by animation name.
    pub keyframes:  HashMap<String, Vec<KeyframeStop>>,
    /// Selector index: rule indices bucketed by the key selector's id/class/tag.
    /// Built lazily before cascade; avoids O(rules) scan per element.
    idx_by_id:    HashMap<String, Vec<usize>>,
    idx_by_class: HashMap<String, Vec<usize>>,
    idx_by_tag:   HashMap<String, Vec<usize>>,
    idx_universal: Vec<usize>,  // rules with * or no specific key selector
    idx_dirty:    bool,
    /// When true, the cascade stores matched CSS rules on each HtmlBox
    /// for inspector display. Off by default to avoid memory overhead.
    pub inspect_mode: bool,
    /// True if any rule has :hover on a non-subject selector part (descendant hover rules).
    /// When true, descendants of hover-changed nodes must also be re-cascaded.
    pub has_hover_descendant_rules: bool,
    /// Raw (comment-stripped) CSS sources, kept for re-extracting variables with viewport.
    pub raw_sources:  Vec<String>,
}

impl Stylesheet {
    pub fn add_rule(&mut self, rule: CssRule) {
        self.rules.push(rule);
        self.idx_dirty = true;
    }

    /// Parse a CSS string and append its rules. Also extracts CSS variables from `:root`.
    /// `css_base_url` is the URL of the CSS file itself, used to resolve relative url()
    /// references (e.g. `url('../image.jpg')` in an external stylesheet).
    pub fn parse_and_add_with_base(&mut self, css: &str, css_base_url: &str) {
        let resolved = resolve_css_urls(css, css_base_url);
        self.parse_and_add(&resolved);
    }

    /// Parse a CSS string from an external `<link>` with a `media` attribute.
    /// When `link_media` is non-empty (e.g. "print"), all parsed rules inherit
    /// that media condition so they're only applied in the matching context.
    pub fn parse_and_add_with_base_media(&mut self, css: &str, css_base_url: &str, link_media: &str) {
        if link_media.is_empty() || link_media.eq_ignore_ascii_case("all") || link_media.eq_ignore_ascii_case("screen") {
            self.parse_and_add_with_base(css, css_base_url);
        } else {
            let before = self.rules.len();
            self.parse_and_add_with_base(css, css_base_url);
            // Tag all newly added rules with the link's media condition
            for rule in &mut self.rules[before..] {
                if rule.media_condition.is_empty() {
                    rule.media_condition = link_media.to_string();
                }
            }
        }
    }

    /// Parse a CSS string and append its rules. Also extracts CSS variables from `:root`.
    pub fn parse_and_add(&mut self, css: &str) {
        // Strip comments once, share the cleaned string across all extractors.
        let cleaned = strip_css_comments(css);
        let cleaned = cleaned.as_str();
        // Extract :root CSS variables. Cross-file resolution is deferred to
        // resolve_variables_for_viewport() which runs after all CSS is loaded.
        extract_root_variables_cleaned(cleaned, &mut self.variables);
        self.raw_sources.push(cleaned.to_string());
        // Extract @font-face declarations
        extract_font_faces_cleaned(cleaned, &mut self.font_faces);
        // Extract @keyframes blocks
        let kf = extract_keyframes_cleaned(cleaned);
        self.keyframes.extend(kf);
        if let Some(rules) = parse_stylesheet_cleaned(cleaned) {
            for r in rules {
                self.rules.push(r);
            }
            self.idx_dirty = true;
        }
    }

    /// Re-extract CSS variables from `:root` with viewport-aware media queries.
    /// Call this before cascade when viewport dimensions are known, so variables
    /// inside `@media` blocks are only extracted when the query matches.
    pub fn resolve_variables_for_viewport(&mut self, vw: f32, vh: f32) {
        self.variables.clear();
        for src in &self.raw_sources {
            extract_root_variables_vp(src, &mut self.variables, vw, vh);
        }
        pre_resolve_variables(&mut self.variables);
    }

    /// Rebuild the selector index if dirty.  Called once before each cascade pass.
    pub fn rebuild_index(&mut self) {
        if !self.idx_dirty { return; }
        self.idx_by_id.clear();
        self.idx_by_class.clear();
        self.idx_by_tag.clear();
        self.idx_universal.clear();
        for (i, rule) in self.rules.iter().enumerate() {
            let keys = rule_key_selectors(rule);
            if keys.is_empty() {
                self.idx_universal.push(i);
            } else {
                for key in keys {
                    match key {
                        SelectorKey::Id(s)    => self.idx_by_id.entry(s).or_default().push(i),
                        SelectorKey::Class(s) => self.idx_by_class.entry(s).or_default().push(i),
                        SelectorKey::Tag(s)   => self.idx_by_tag.entry(s).or_default().push(i),
                        SelectorKey::Universal => self.idx_universal.push(i),
                    }
                }
            }
        }
        self.idx_dirty = false;

        // Pre-compile declarations (string → PropertyId) for fast cascade dispatch
        for rule in &mut self.rules {
            rule.compile_declarations();
        }

        // Detect if any rule has :hover on a non-subject part (descendant hover selectors).
        // e.g., ".parent:hover .child" — :hover is on .parent (ancestor), not .child (subject).
        self.has_hover_descendant_rules = false;
        'rules: for rule in &self.rules {
            if !rule.is_hover { continue; }
            for sel in &rule.selectors {
                // Find the last combinator — everything before it is ancestor context
                let last_comb = sel.parts.iter().rposition(|p| matches!(p, SelectorPart::Combinator(_)));
                if let Some(pos) = last_comb {
                    // Check if :hover appears in the ancestor part (before the combinator)
                    for part in &sel.parts[..pos] {
                        if matches!(part, SelectorPart::PseudoClass(ref pc) if pc == "hover") {
                            self.has_hover_descendant_rules = true;
                            break 'rules;
                        }
                    }
                }
            }
        }
    }

    /// Get candidate rule indices for an element with given tag, id, and classes.
    /// Writes into a reusable buffer to avoid per-element allocation.
    pub fn candidate_rules(&self, tag: &str, id: Option<&str>, classes: &[&str], out: &mut Vec<usize>) {
        out.clear();
        // Add universal rules (always candidates)
        out.extend_from_slice(&self.idx_universal);
        // Add tag-matched rules
        let tag_lower = tag.to_ascii_lowercase();
        if let Some(indices) = self.idx_by_tag.get(&tag_lower) {
            out.extend_from_slice(indices);
        }
        // Add id-matched rules
        if let Some(id) = id {
            if let Some(indices) = self.idx_by_id.get(id) {
                out.extend_from_slice(indices);
            }
        }
        // Add class-matched rules
        for cls in classes {
            if let Some(indices) = self.idx_by_class.get(*cls) {
                out.extend_from_slice(indices);
            }
        }
        // Use a fast dedup via a seen-bitset instead of sort+dedup.
        // For typical pages (< 10k rules), a bitset is much faster than sorting.
        if out.len() > 1 {
            let max_idx = out.iter().copied().max().unwrap_or(0);
            if max_idx < 65536 {
                // Fast path: bitvec dedup
                let words = (max_idx / 64) + 1;
                let mut seen = vec![0u64; words];
                let mut write = 0;
                for read in 0..out.len() {
                    let idx = out[read];
                    let word = idx / 64;
                    let bit = 1u64 << (idx % 64);
                    if seen[word] & bit == 0 {
                        seen[word] |= bit;
                        out[write] = idx;
                        write += 1;
                    }
                }
                out.truncate(write);
            } else {
                out.sort_unstable();
                out.dedup();
            }
        }
    }
}

/// Key extracted from the rightmost simple selector of a rule.
enum SelectorKey {
    Id(String),
    Class(String),
    Tag(String),
    Universal,
}

/// Extract key selectors from ALL selectors in a rule (handles comma-separated selectors).
/// Each selector produces one key (id > class > tag > universal).
fn rule_key_selectors(rule: &CssRule) -> Vec<SelectorKey> {
    let mut keys = Vec::new();
    for sel in &rule.selectors {
        // Walk from right to left, skip combinators, find the rightmost id/class/tag.
        let mut best_id = None;
        let mut best_class = None;
        let mut best_tag = None;
        for part in sel.parts.iter().rev() {
            match part {
                SelectorPart::Combinator(_) => break, // stop at first combinator from right
                SelectorPart::Id(s)    => { best_id = Some(s.clone()); break; }
                SelectorPart::Class(s) => { if best_class.is_none() { best_class = Some(s.clone()); } }
                SelectorPart::Tag(t) if t != "*" => { if best_tag.is_none() { best_tag = Some(t.to_ascii_lowercase()); } }
                _ => {}
            }
        }
        let key = if let Some(id) = best_id {
            SelectorKey::Id(id)
        } else if let Some(cls) = best_class {
            SelectorKey::Class(cls)
        } else if let Some(tag) = best_tag {
            SelectorKey::Tag(tag)
        } else {
            SelectorKey::Universal
        };
        keys.push(key);
    }
    keys
}

// ─── @keyframes extraction ────────────────────────────────────────────────────

/// Parse all `@keyframes` (and `@-webkit-keyframes`) blocks from a CSS string.
pub fn extract_keyframes(css: &str) -> HashMap<String, Vec<KeyframeStop>> {
    let cleaned = strip_css_comments(css);
    extract_keyframes_cleaned(&cleaned)
}

fn extract_keyframes_cleaned(css: &str) -> HashMap<String, Vec<KeyframeStop>> {
    let mut out: HashMap<String, Vec<KeyframeStop>> = HashMap::new();
    let mut s = css.trim();

    while !s.is_empty() {
        s = s.trim_start();
        if s.is_empty() { break; }

        if !s.starts_with('@') {
            // Skip regular rules without recursing
            if let Some(brace) = s.find('{') {
                let (_, rest) = consume_block(&s[brace..]);
                s = rest;
            } else { break; }
            continue;
        }

        // Only lowercase a small prefix to identify the @-rule type
        let prefix = &s[..s.len().min(30)];
        let at_lower: String = prefix.to_ascii_lowercase();

        // Handle no-block @ rules
        if at_lower.starts_with("@import") || at_lower.starts_with("@charset") {
            if let Some(semi) = s.find(';') { s = &s[semi + 1..]; } else { break; }
            continue;
        }

        let brace = match s.find('{') {
            Some(p) => p,
            None => { if let Some(semi) = s.find(';') { s = &s[semi+1..]; } else { break; } continue; }
        };
        let at_header = s[..brace].trim();
        let rest_from_brace = &s[brace..];
        let (inner_block, after_block) = consume_block(rest_from_brace);

        if at_lower.starts_with("@keyframes") || at_lower.starts_with("@-webkit-keyframes") {
            let prefix_len = if at_lower.starts_with("@-webkit-keyframes") {
                "@-webkit-keyframes".len()
            } else {
                "@keyframes".len()
            };
            let name = at_header[prefix_len..].trim().to_string();
            if !name.is_empty() {
                out.insert(name, parse_keyframe_stops(inner_block));
            }
        } else if at_lower.starts_with("@media") || at_lower.starts_with("@container") {
            // Recurse for nested @keyframes (rare but spec-valid)
            out.extend(extract_keyframes_cleaned(inner_block));
        }

        s = after_block;
    }
    out
}

/// Parse the body of a `@keyframes` block into a sorted list of stops.
fn parse_keyframe_stops(block: &str) -> Vec<KeyframeStop> {
    let mut stops: Vec<KeyframeStop> = Vec::new();
    let mut s = block.trim();

    while !s.is_empty() {
        s = s.trim_start();
        if s.is_empty() { break; }

        let brace = match s.find('{') { Some(p) => p, None => break };
        let selector = s[..brace].trim();
        let (decl_block, rest) = consume_block(&s[brace..]);
        s = rest;

        let props = parse_declarations(decl_block);
        let prop_vec: Vec<(String, String)> = props.into_iter().map(|(k, v)| {
            // Normalize color values to rgba() so interpolation works.
            let is_color_prop = matches!(k.as_str(),
                "color" | "background-color" | "border-color" |
                "border-top-color" | "border-right-color" |
                "border-bottom-color" | "border-left-color" |
                "outline-color" | "fill" | "stroke"
            );
            if is_color_prop {
                if let Some(c) = parse_color(&v) {
                    return (k, format!("rgba({},{},{},{})", c.r, c.g, c.b,
                        (c.a as f32 / 255.0 * 1000.0).round() / 1000.0));
                }
            }
            (k, v)
        }).collect();

        for sel in selector.split(',') {
            let sel = sel.trim();
            let offset: f32 = match sel {
                "from" => 0.0,
                "to"   => 1.0,
                s if s.ends_with('%') => {
                    s[..s.len()-1].trim().parse::<f32>().unwrap_or(0.0) / 100.0
                }
                _ => continue,
            };
            stops.push(KeyframeStop { offset, properties: prop_vec.clone() });
        }
    }

    stops.sort_by(|a, b| a.offset.partial_cmp(&b.offset).unwrap_or(std::cmp::Ordering::Equal));
    stops
}

// ─── Animation / transition shorthand parsers ─────────────────────────────────

/// Parse an `animation-timing-function` value into an `EasingFn`.
pub fn parse_easing(s: &str) -> EasingFn {
    let s = s.trim();
    match s {
        "linear"      => EasingFn::Linear,
        "ease"        => EasingFn::Ease,
        "ease-in"     => EasingFn::EaseIn,
        "ease-out"    => EasingFn::EaseOut,
        "ease-in-out" => EasingFn::EaseInOut,
        "step-start"  => EasingFn::StepStart,
        "step-end"    => EasingFn::StepEnd,
        s if s.starts_with("cubic-bezier(") => {
            let inner = s.strip_prefix("cubic-bezier(").unwrap_or("")
                         .strip_suffix(')').unwrap_or("");
            let parts: Vec<f32> = inner.split(',')
                .filter_map(|p| p.trim().parse().ok()).collect();
            if parts.len() == 4 { EasingFn::CubicBezier(parts[0], parts[1], parts[2], parts[3]) }
            else { EasingFn::Ease }
        }
        s if s.starts_with("steps(") => {
            let inner = s.strip_prefix("steps(").unwrap_or("")
                         .strip_suffix(')').unwrap_or("");
            let mut it = inner.splitn(2, ',');
            let count = it.next().and_then(|p| p.trim().parse::<u32>().ok()).unwrap_or(1);
            let jump  = it.next()
                .map(|p| matches!(p.trim(), "start" | "jump-start"))
                .unwrap_or(false);
            EasingFn::Steps(count, jump)
        }
        _ => EasingFn::Ease,
    }
}

/// Parse an `animation` shorthand value (comma-separated list of animations).
pub fn parse_animation_shorthand(s: &str) -> Vec<ParsedAnimation> {
    s.split(',').filter_map(|part| parse_single_animation(part.trim())).collect()
}

fn parse_single_animation(s: &str) -> Option<ParsedAnimation> {
    let mut name              = String::new();
    let mut duration_ms       = 0.0f32;
    let mut delay_ms          = 0.0f32;
    let mut timing_fn         = EasingFn::Ease;
    let mut iteration_count   = 1.0f32;
    let mut direction         = AnimDirection::Normal;
    let mut fill_mode         = FillMode::None;
    let mut play_state_paused = false;
    let mut got_duration      = false;

    for tok in tokenize_anim(s) {
        if tok.is_empty() { continue; }
        if let Some(ms) = parse_time_ms(&tok) {
            if !got_duration { duration_ms = ms; got_duration = true; }
            else             { delay_ms = ms; }
            continue;
        }
        if is_timing_fn(&tok) { timing_fn = parse_easing(&tok); continue; }
        match tok.as_str() {
            "normal"           => { direction = AnimDirection::Normal; continue; }
            "reverse"          => { direction = AnimDirection::Reverse; continue; }
            "alternate"        => { direction = AnimDirection::Alternate; continue; }
            "alternate-reverse"=> { direction = AnimDirection::AlternateReverse; continue; }
            "none"             => { fill_mode = FillMode::None; continue; }
            "forwards"         => { fill_mode = FillMode::Forwards; continue; }
            "backwards"        => { fill_mode = FillMode::Backwards; continue; }
            "both"             => { fill_mode = FillMode::Both; continue; }
            "running"          => { play_state_paused = false; continue; }
            "paused"           => { play_state_paused = true; continue; }
            "infinite"         => { iteration_count = f32::INFINITY; continue; }
            _ => {}
        }
        if let Ok(n) = tok.parse::<f32>() { iteration_count = n; continue; }
        if name.is_empty() { name = tok.clone(); }
    }

    if name.is_empty() || name == "none" { return None; }
    Some(ParsedAnimation { name, duration_ms, delay_ms, timing_fn,
                           iteration_count, direction, fill_mode, play_state_paused })
}

/// Parse a `transition` shorthand value (comma-separated list of transitions).
pub fn parse_transition_shorthand(s: &str) -> Vec<ParsedTransition> {
    s.split(',').filter_map(|part| parse_single_transition(part.trim())).collect()
}

fn parse_single_transition(s: &str) -> Option<ParsedTransition> {
    let mut property    = String::new();
    let mut duration_ms = 0.0f32;
    let mut delay_ms    = 0.0f32;
    let mut timing_fn   = EasingFn::Ease;
    let mut got_duration = false;

    for tok in tokenize_anim(s) {
        if tok.is_empty() { continue; }
        if let Some(ms) = parse_time_ms(&tok) {
            if !got_duration { duration_ms = ms; got_duration = true; }
            else             { delay_ms = ms; }
            continue;
        }
        if is_timing_fn(&tok) { timing_fn = parse_easing(&tok); continue; }
        if property.is_empty() { property = tok.clone(); }
    }

    if property.is_empty() || property == "none" { return None; }
    Some(ParsedTransition { property, duration_ms, delay_ms, timing_fn })
}

/// Split an animation/transition shorthand token (handles `cubic-bezier(…)` as one token).
fn tokenize_anim(s: &str) -> Vec<String> {
    let mut tokens  = Vec::new();
    let mut current = String::new();
    let mut depth   = 0usize;
    for ch in s.chars() {
        match ch {
            '(' => { depth += 1; current.push(ch); }
            ')' => { depth = depth.saturating_sub(1); current.push(ch); }
            ' ' | '\t' if depth == 0 => {
                if !current.is_empty() { tokens.push(current.clone()); current.clear(); }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() { tokens.push(current); }
    tokens
}

pub fn parse_time_ms(s: &str) -> Option<f32> {
    if let Some(ms)  = s.strip_suffix("ms") { ms.trim().parse::<f32>().ok() }
    else if let Some(sec) = s.strip_suffix('s') { sec.trim().parse::<f32>().ok().map(|v| v * 1000.0) }
    else { None }
}

fn is_timing_fn(s: &str) -> bool {
    matches!(s, "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" | "step-start" | "step-end")
    || s.starts_with("cubic-bezier(")
    || s.starts_with("steps(")
}

/// Extract CSS custom properties (--name: value) from `:root { }` blocks.
/// Extract CSS custom properties (--*) from rule blocks.
/// Collects from any rule block, since custom properties can be set on any element
/// and are inherited. This matches the common patterns: `:root`, `html`, `html.class`,
/// `body`, and element-level overrides.
fn extract_root_variables(css: &str, vars: &mut HashMap<String, String>) {
    let cleaned = strip_css_comments(css);
    extract_root_variables_cleaned(&cleaned, vars);
}

fn extract_root_variables_cleaned(css: &str, vars: &mut HashMap<String, String>) {
    extract_root_variables_inner(css, vars);
}

fn extract_root_variables_inner(css: &str, vars: &mut HashMap<String, String>) {
    extract_root_variables_vp(css, vars, 0.0, 0.0);
}

fn extract_root_variables_vp(css: &str, vars: &mut HashMap<String, String>, vw: f32, vh: f32) {
    let mut s = css;
    while !s.is_empty() {
        s = s.trim_start();
        if s.is_empty() { break; }
        if s.starts_with('@') {
            let prefix = &s[..s.len().min(30)];
            let lower: String = prefix.to_ascii_lowercase();
            if lower.starts_with("@media") {
                if let Some(brace) = s.find('{') {
                    // Only extract variables from matching media queries
                    // (when viewport is known, i.e. vw > 0).
                    let condition = s[6..brace].trim();
                    let matches = vw == 0.0 || evaluate_media(condition, vw, vh);
                    let (block, rest) = consume_block(&s[brace..]);
                    if matches {
                        extract_root_variables_vp(&block, vars, vw, vh);
                    }
                    s = rest;
                } else { break; }
                continue;
            }
            // @layer, @supports: recurse into their blocks to find :root variables
            if lower.starts_with("@layer") || lower.starts_with("@supports") {
                if let Some(brace) = s.find('{') {
                    let (block, rest) = consume_block(&s[brace..]);
                    extract_root_variables_vp(&block, vars, vw, vh);
                    s = rest;
                } else { break; }
                continue;
            }
            // Other @-rules (@keyframes, @font-face, etc.): skip
            if let Some(brace) = s.find('{') {
                let (_, rest) = consume_block(&s[brace..]);
                s = rest;
            } else { break; }
            continue;
        }
        // Skip stray closing braces (from minified CSS where blocks run together)
        if s.starts_with('}') {
            s = &s[1..];
            continue;
        }
        if let Some(brace) = s.find('{') {
            let selector = s[..brace].trim();
            let (block, rest) = consume_block(&s[brace..]);
            // Only extract custom properties from :root and html selectors
            // (universal scope). Selector-specific variables are handled
            // by the per-element cascade via inherited_vars.
            let sel_lower = selector.to_ascii_lowercase();
            let is_root = sel_lower.split(',').any(|s| {
                let s = s.trim();
                s == ":root" || s == "html" || s == "*"
                    || s.starts_with(":root ") || s.starts_with(":root,")
                    || s.starts_with("html ") || s.starts_with("html,")
                    || s.starts_with("html[")
            });
            if is_root && block.contains("--") {
                for decl in block.split(';') {
                    let decl = decl.trim();
                    if let Some(colon) = decl.find(':') {
                        let prop = decl[..colon].trim();
                        if prop.starts_with("--") {
                            let val = decl[colon+1..].trim().to_string();
                            // Don't overwrite a non-empty value with an empty one
                            // (prevents dark-mode selectors from clobbering light-mode defaults)
                            if !val.is_empty() || !vars.contains_key(prop) {
                                vars.insert(prop.to_string(), val);
                            }
                        }
                    }
                }
            }
            s = rest;
        } else { break; }
    }
}

/// Expand `var()` references within the variable map itself so all values are concrete.
/// Handles chains (--a: var(--b), --b: 1rem) and circular refs (uses fallback or "").
fn pre_resolve_variables(vars: &mut HashMap<String, String>) {
    // Handle csstools light-dark() polyfill: in light mode (our default),
    // the toggle variables should be empty so fallback (light) values are used.
    // The polyfill sets --csstools-color-scheme--light: initial in light mode,
    // which makes --csstools-light-dark-toggle--N invalid → fallback kicks in.
    // We simulate this by removing the toggle variables entirely.
    let toggle_keys: Vec<String> = vars.keys()
        .filter(|k| k.starts_with("--csstools-light-dark-toggle-"))
        .cloned()
        .collect();
    for key in &toggle_keys {
        vars.remove(key);
    }

    let keys: Vec<String> = vars.keys().cloned().collect();
    let max_passes = keys.len().min(50);
    for _ in 0..max_passes {
        let mut changed = false;
        let snapshot = vars.clone();
        for key in &keys {
            if let Some(val) = vars.get(key) {
                if val.contains("var(") {
                    let resolved = resolve_var_pass(val, &snapshot);
                    if resolved != *val {
                        vars.insert(key.clone(), resolved);
                        changed = true;
                    }
                }
            }
        }
        if !changed { break; }
    }
    // Final pass: replace any still-unresolved var() with their fallback or "".
    let keys: Vec<String> = vars.keys().cloned().collect();
    for key in &keys {
        if let Some(val) = vars.get(key) {
            let mut resolved = val.clone();
            if resolved.contains("var(") {
                resolved = resolve_var_pass(&resolved, &HashMap::new());
            }
            // Resolve light-dark() → use light value
            if resolved.contains("light-dark(") {
                if let Some(start) = resolved.find("light-dark(") {
                    let inner = &resolved[start + 11..];
                    if let Some(comma) = inner.find(',') {
                        if let Some(end) = inner.rfind(')') {
                            let light_val = inner[..comma].trim().to_string();
                            resolved = format!("{}{}{}", &resolved[..start], light_val, &inner[end+1..]);
                        }
                    }
                }
            }
            if resolved != *val {
                vars.insert(key.clone(), resolved);
            }
        }
    }
}

/// Extract @font-face declarations from a CSS string.
pub fn extract_font_faces(css: &str, faces: &mut Vec<FontFaceDecl>) {
    let cleaned = strip_css_comments(css);
    extract_font_faces_cleaned(&cleaned, faces);
}

/// Split CSS declarations on `;`, but skip `;` inside parentheses (for data URIs).
fn split_declarations_paren_aware(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => { if depth > 0 { depth -= 1; } }
            ';' if depth == 0 => {
                result.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        result.push(&s[start..]);
    }
    result
}

fn extract_font_faces_cleaned(css: &str, faces: &mut Vec<FontFaceDecl>) {
    let mut s = css;
    loop {
        s = s.trim_start();
        if s.is_empty() { break; }
        // Search for @font-face case-insensitively without lowercasing entire string
        let pos = match find_case_insensitive(s, "@font-face") {
            Some(p) => p,
            None    => break,
        };
        s = &s[pos + 10..];
        s = s.trim_start();
        if !s.starts_with('{') { continue; }
        let (block, rest) = consume_block(s);
        s = rest;

        // Parse declarations — split on `;` outside parentheses (to avoid
        // splitting inside `url(data:...;base64,...)`)
        let mut face = FontFaceDecl::default();
        for decl in split_declarations_paren_aware(block) {
            let decl = decl.trim();
            if let Some(colon) = decl.find(':') {
                let prop  = decl[..colon].trim().to_ascii_lowercase();
                let value = decl[colon+1..].trim().to_string();
                match prop.as_str() {
                    "font-family" => {
                        face.family = value.trim_matches('"').trim_matches('\'').to_string();
                    }
                    "src" => {
                        face.src = value;
                    }
                    "font-weight" => { face.weight = Some(value); }
                    "font-style"  => { face.style  = Some(value); }
                    _ => {}
                }
            }
        }
        if !face.family.is_empty() || !face.src.is_empty() {
            faces.push(face);
        }
    }
}

// ─── CSS Parser ──────────────────────────────────────────────────────────────

/// Parse a full stylesheet text into rules.
/// `parent_media` is non-empty when called recursively from inside an @media block.
pub fn parse_stylesheet(css: &str) -> Option<Vec<CssRule>> {
    let cleaned = strip_css_comments(css);
    parse_stylesheet_cleaned(&cleaned)
}

fn parse_stylesheet_cleaned(css: &str) -> Option<Vec<CssRule>> {
    parse_stylesheet_inner(css, "")
}

fn parse_stylesheet_inner(css: &str, parent_media: &str) -> Option<Vec<CssRule>> {
    let mut rules = Vec::new();
    let mut s = css.trim();

    while !s.is_empty() {
        s = s.trim_start();
        if s.is_empty() { break; }

        // @rules
        if s.starts_with('@') {
            // Only lowercase a small prefix (enough to identify the @-rule type)
            let prefix_len = s.len().min(30);
            let at_lower: String = s[..prefix_len].to_ascii_lowercase();

            // @import / @charset — skip to semicolon (no block)
            if at_lower.starts_with("@import") || at_lower.starts_with("@charset") {
                if let Some(semi) = s.find(';') {
                    s = &s[semi + 1..];
                } else { break; }
                continue;
            }

            // Find the opening brace
            let brace = match s.find('{') {
                Some(p) => p,
                None    => { if let Some(semi) = s.find(';') { s = &s[semi+1..]; } else { break; } continue; }
            };
            let at_header = s[..brace].trim();
            let rest_from_brace = &s[brace..];
            let (inner_block, after_block) = consume_block(rest_from_brace);

            if at_lower.starts_with("@media") {
                // Extract condition: everything after "@media"
                let condition = at_header[6..].trim();
                let media_cond = if parent_media.is_empty() {
                    condition.to_string()
                } else {
                    format!("{} and {}", parent_media, condition)
                };
                // Recursively parse inner block
                if let Some(inner_rules) = parse_stylesheet_inner(inner_block, &media_cond) {
                    for r in inner_rules { rules.push(r); }
                }
            } else if at_lower.starts_with("@container") {
                // @container [name] (condition) { ... }
                // Extract optional container name and condition string.
                let header = at_header["@container".len()..].trim();
                let (cname, cond) = if header.starts_with('(') {
                    (String::new(), header.to_string())
                } else if let Some(paren) = header.find('(') {
                    (header[..paren].trim().to_string(), header[paren..].trim().to_string())
                } else {
                    (String::new(), header.to_string())
                };
                if let Some(mut inner_rules) = parse_stylesheet_inner(inner_block, parent_media) {
                    for r in &mut inner_rules {
                        r.container_condition = cond.clone();
                        r.container_name      = cname.clone();
                    }
                    for r in inner_rules { rules.push(r); }
                }
            } else if at_lower.starts_with("@supports") {
                // @supports — parse inner rules (assume all features are supported)
                if let Some(inner_rules) = parse_stylesheet_inner(inner_block, parent_media) {
                    for r in inner_rules { rules.push(r); }
                }
            } else if at_lower.starts_with("@layer") {
                // @layer — parse inner rules (ignore layer ordering for now)
                if let Some(inner_rules) = parse_stylesheet_inner(inner_block, parent_media) {
                    for r in inner_rules { rules.push(r); }
                }
            }
            // else: @keyframes, @font-face, etc. — skip the block

            s = after_block;
            continue;
        }

        // Selector(s) { declarations }
        let brace_pos = match s.find('{') {
            Some(p) => p,
            None    => break,
        };

        let selector_text = s[..brace_pos].trim();
        let (decl_block, rest) = consume_block(&s[brace_pos..]);
        s = rest;

        let (declarations, important_declarations) = parse_declarations_important(decl_block);
        if declarations.is_empty() && important_declarations.is_empty() { continue; }

        // Split comma-separated selectors (respecting parentheses)
        for sel_str in split_selectors(selector_text) {
            let sel_str = sel_str.trim();
            if sel_str.is_empty() { continue; }

            // :root — extract CSS variables
            if sel_str == ":root" {
                // Variables are stored on the Stylesheet, not as rules.
                // We emit a special rule with empty selectors as a marker;
                // the caller (parse_and_add) handles it.
                // For now: skip (variables handled by Stylesheet::parse_and_add).
                continue;
            }

            let original_selector = sel_str.to_string();

            // Detect ::before / ::after pseudo-elements, strip from selector for matching
            let (sel_for_match, pseudo_elem) = strip_pseudo_element(sel_str);

            let sel = parse_selector(&sel_for_match);
            let sp  = sel.specificity();

            // Detect :hover in selector parts
            let is_hover = sel.parts.iter().any(|p| {
                matches!(p, SelectorPart::PseudoClass(name) if name == "hover")
            });

            let mut rule = CssRule::default();
            rule.selectors        = vec![sel];
            rule.declarations     = declarations.clone();
            rule.important_declarations = important_declarations.clone();
            rule.specificity      = sp;
            rule.media_condition  = parent_media.to_string();
            rule.original_selector = original_selector;
            rule.is_hover         = is_hover;
            rule.pseudo_element   = pseudo_elem;
            rules.push(rule);
        }
    }

    Some(rules)
}

/// Strip `/* ... */` comments from CSS text.
fn strip_css_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut i = 0;
    let bytes = css.as_bytes();
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i+1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i+1] == b'/') { i += 1; }
            if i + 1 < bytes.len() { i += 2; }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Case-insensitive substring search without allocating a lowercased copy.
pub fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let needle_bytes = needle.as_bytes();
    let nlen = needle_bytes.len();
    if nlen == 0 { return Some(0); }
    let hbytes = haystack.as_bytes();
    if hbytes.len() < nlen { return None; }
    'outer: for i in 0..=(hbytes.len() - nlen) {
        for j in 0..nlen {
            if hbytes[i + j].to_ascii_lowercase() != needle_bytes[j].to_ascii_lowercase() {
                continue 'outer;
            }
        }
        return Some(i);
    }
    None
}

/// Detect and strip `::before` / `::after` (and CSS2 `:before`/`:after`) from
/// a selector string.  Returns (cleaned_selector, PseudoElement).
fn strip_pseudo_element(sel: &str) -> (String, PseudoElement) {
    // :: double-colon pseudo-elements
    if let Some(pos) = sel.find("::") {
        let pe_str = sel[pos+2..].to_ascii_lowercase();
        let (kw_len, pe) =
            if pe_str.starts_with("before")       { (6,  PseudoElement::Before)    }
            else if pe_str.starts_with("after")    { (5,  PseudoElement::After)     }
            else if pe_str.starts_with("selection"){ (9,  PseudoElement::Selection) }
            else if pe_str.starts_with("marker")   { (6,  PseudoElement::Marker)    }
            else if pe_str.starts_with("first-line")    { (10, PseudoElement::Ignored) }
            else if pe_str.starts_with("first-letter")  { (12, PseudoElement::Ignored) }
            else if pe_str.starts_with("placeholder")   { (11, PseudoElement::Ignored) }
            else {
                // Unknown vendor or other pseudo-element — ignore rule entirely
                return (String::new(), PseudoElement::Ignored);
            };
        let clean = format!("{}{}", &sel[..pos], &sel[pos+2+kw_len..]).trim().to_string();
        let clean = if clean.is_empty() { "*".to_string() } else { clean };
        return (clean, pe);
    }
    // CSS2 single-colon :before / :after (not preceded by another colon)
    let sel_lower = sel.to_ascii_lowercase();
    for (kw, pe) in &[(":before", PseudoElement::Before), (":after", PseudoElement::After)] {
        if let Some(pos) = sel_lower.find(kw) {
            if pos > 0 && sel.as_bytes()[pos-1] == b':' { continue; }
            let clean = format!("{}{}", &sel[..pos], &sel[pos+kw.len()..]).trim().to_string();
            let clean = if clean.is_empty() { "*".to_string() } else { clean };
            return (clean, pe.clone());
        }
    }
    (sel.to_string(), PseudoElement::None)
}

fn consume_block(s: &str) -> (&str, &str) {
    // s starts with '{'
    let mut depth = 0usize;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return (&s[1..i], &s[i + 1..]);
                }
            }
            _ => {}
        }
        i += 1;
    }
    (s, "")
}

/// Parse "prop: value; prop: value; ..." into a map.
/// Strips `!important` from values.
pub fn parse_declarations(block: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for decl in block.split(';') {
        let decl = decl.trim();
        if decl.is_empty() { continue; }
        if let Some(colon) = decl.find(':') {
            let raw_prop = decl[..colon].trim();
            // CSS custom properties (--*) are case-sensitive; standard properties are not.
            let prop = if raw_prop.starts_with("--") {
                raw_prop.to_string()
            } else {
                raw_prop.to_ascii_lowercase()
            };
            let value = strip_important(decl[colon + 1..].trim());
            if !prop.is_empty() && !value.is_empty() {
                map.insert(prop, value);
            }
        }
    }
    map
}

/// Parse declarations, splitting into (normal, important) maps.
/// Properties with `!important` go into the second map.
pub fn parse_declarations_important(block: &str) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut normal = HashMap::new();
    let mut important = HashMap::new();
    for decl in block.split(';') {
        let decl = decl.trim();
        if decl.is_empty() { continue; }
        if let Some(colon) = decl.find(':') {
            let raw_prop = decl[..colon].trim();
            // CSS custom properties (--*) are case-sensitive; standard properties are not.
            let prop = if raw_prop.starts_with("--") {
                raw_prop.to_string()
            } else {
                raw_prop.to_ascii_lowercase()
            };
            let raw_value = decl[colon + 1..].trim();
            let is_important = has_important(raw_value);
            let value = strip_important(raw_value);
            if !prop.is_empty() && !value.is_empty() {
                if is_important {
                    important.insert(prop, value);
                } else {
                    normal.insert(prop, value);
                }
            }
        }
    }
    (normal, important)
}

/// Check if a CSS value contains `!important` (with optional whitespace).
fn has_important(val: &str) -> bool {
    // Match !important, ! important, !  important etc.
    if let Some(bang) = val.rfind('!') {
        val[bang + 1..].trim().eq_ignore_ascii_case("important")
    } else {
        false
    }
}

/// Strip `!important` (with optional whitespace) from a CSS value.
fn strip_important(val: &str) -> String {
    if let Some(bang) = val.rfind('!') {
        let after = val[bang + 1..].trim();
        if after.eq_ignore_ascii_case("important") {
            return val[..bang].trim().to_string();
        }
    }
    val.to_string()
}

/// Parse a single CSS selector string into a CssSelector.
pub fn parse_selector(s: &str) -> CssSelector {
    let mut parts = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '\n' => {
                // Consume all leading whitespace
                while matches!(chars.peek(), Some(' ') | Some('\t') | Some('\n')) {
                    chars.next();
                }
                // Determine combinator based on the next non-whitespace character
                let next_non_ws = chars.peek().copied();
                match next_non_ws {
                    Some('>') => {
                        chars.next();
                        // Skip any whitespace after the '>'
                        while matches!(chars.peek(), Some(' ') | Some('\t') | Some('\n')) {
                            chars.next();
                        }
                        parts.push(SelectorPart::Combinator(Combinator::Child));
                    }
                    Some('+') => {
                        chars.next();
                        while matches!(chars.peek(), Some(' ') | Some('\t') | Some('\n')) {
                            chars.next();
                        }
                        parts.push(SelectorPart::Combinator(Combinator::AdjacentSibling));
                    }
                    Some('~') => {
                        chars.next();
                        while matches!(chars.peek(), Some(' ') | Some('\t') | Some('\n')) {
                            chars.next();
                        }
                        parts.push(SelectorPart::Combinator(Combinator::GeneralSibling));
                    }
                    _ => { parts.push(SelectorPart::Combinator(Combinator::Descendant)); }
                }
            }
            '>' => { chars.next(); parts.push(SelectorPart::Combinator(Combinator::Child)); }
            '+' => { chars.next(); parts.push(SelectorPart::Combinator(Combinator::AdjacentSibling)); }
            '~' => { chars.next(); parts.push(SelectorPart::Combinator(Combinator::GeneralSibling)); }
            '#' => {
                chars.next();
                let id = read_ident(&mut chars);
                parts.push(SelectorPart::Id(id));
            }
            '.' => {
                chars.next();
                let cls = read_ident(&mut chars);
                parts.push(SelectorPart::Class(cls));
            }
            ':' => {
                chars.next();
                let is_elem = chars.peek() == Some(&':');
                if is_elem { chars.next(); }
                let name = read_ident(&mut chars);
                // consume optional (...)
                if chars.peek() == Some(&'(') {
                    // Collect balanced args (respecting nested parens)
                    chars.next(); // consume '('
                    let args = read_balanced_parens(&mut chars);
                    if !is_elem {
                        match name.as_str() {
                            "not" => {
                                let selectors: Vec<CssSelector> = args.split(',')
                                    .map(|s| parse_selector(s.trim()))
                                    .collect();
                                if selectors.len() == 1 {
                                    parts.push(SelectorPart::Not(Box::new(selectors.into_iter().next().unwrap())));
                                } else {
                                    // :not(.a,.b) ≡ :not(.a):not(.b)
                                    for sel in selectors {
                                        parts.push(SelectorPart::Not(Box::new(sel)));
                                    }
                                }
                            }
                            "is" => {
                                let selectors: Vec<CssSelector> = args.split(',')
                                    .map(|s| parse_selector(s.trim()))
                                    .collect();
                                parts.push(SelectorPart::Is(selectors));
                            }
                            "where" => {
                                let selectors: Vec<CssSelector> = args.split(',')
                                    .map(|s| parse_selector(s.trim()))
                                    .collect();
                                parts.push(SelectorPart::Where(selectors));
                            }
                            "has" => {
                                let inner_sel = parse_selector(args.trim());
                                parts.push(SelectorPart::Has(Box::new(inner_sel)));
                            }
                            _ => {
                                let full_name = format!("{}({})", name, args);
                                parts.push(SelectorPart::PseudoClass(full_name));
                            }
                        }
                    } else {
                        let full_name = format!("{}({})", name, args);
                        parts.push(SelectorPart::PseudoElement(full_name));
                    }
                } else if is_elem {
                    parts.push(SelectorPart::PseudoElement(name));
                } else {
                    parts.push(SelectorPart::PseudoClass(name));
                }
            }
            '[' => {
                chars.next();
                let attr_str: String = chars.by_ref().take_while(|&c| c != ']').collect();
                let (name, op, value) = parse_attr_selector(&attr_str);
                parts.push(SelectorPart::Attribute { name, op, value });
            }
            '*' => {
                chars.next();
                parts.push(SelectorPart::Universal);
            }
            _ => {
                let tag = read_ident(&mut chars);
                if !tag.is_empty() {
                    parts.push(SelectorPart::Tag(tag.to_ascii_lowercase()));
                } else {
                    chars.next(); // skip unknown
                }
            }
        }
    }

    CssSelector::new(parts)
}

fn read_ident(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut s = String::new();
    while let Some(&c) = chars.peek() {
        if c == '\\' {
            // CSS escape sequence: consume backslash and next character
            chars.next();
            if let Some(&escaped) = chars.peek() {
                s.push(escaped);
                chars.next();
            }
        } else if c.is_alphanumeric() || c == '-' || c == '_' {
            s.push(c);
            chars.next();
        } else {
            break;
        }
    }
    s
}

/// Split a selector list at commas, respecting parentheses nesting.
/// e.g. "body:not(.a,.b) .c, div" → ["body:not(.a,.b) .c", " div"]
fn split_selectors(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => { if depth > 0 { depth -= 1; } }
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// Consume characters until the matching `)` for an already-consumed `(`.
/// Handles nested parens. Returns the content (without the outer parens).
fn read_balanced_parens(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut s = String::new();
    let mut depth = 1usize;
    for c in chars.by_ref() {
        match c {
            '(' => { depth += 1; s.push(c); }
            ')' => {
                depth -= 1;
                if depth == 0 { break; }
                s.push(c);
            }
            _ => { s.push(c); }
        }
    }
    s
}

fn parse_attr_selector(s: &str) -> (String, AttrOp, String) {
    if let Some(op_pos) = s.find("~=") {
        return (s[..op_pos].trim().to_string(), AttrOp::Includes, strip_quotes(&s[op_pos+2..].trim()));
    }
    if let Some(op_pos) = s.find("|=") {
        return (s[..op_pos].trim().to_string(), AttrOp::DashMatch, strip_quotes(&s[op_pos+2..].trim()));
    }
    if let Some(op_pos) = s.find("^=") {
        return (s[..op_pos].trim().to_string(), AttrOp::StartsWith, strip_quotes(&s[op_pos+2..].trim()));
    }
    if let Some(op_pos) = s.find("$=") {
        return (s[..op_pos].trim().to_string(), AttrOp::EndsWith, strip_quotes(&s[op_pos+2..].trim()));
    }
    if let Some(op_pos) = s.find("*=") {
        return (s[..op_pos].trim().to_string(), AttrOp::Contains, strip_quotes(&s[op_pos+2..].trim()));
    }
    if let Some(op_pos) = s.find('=') {
        return (s[..op_pos].trim().to_string(), AttrOp::Eq, strip_quotes(&s[op_pos+1..].trim()));
    }
    (s.trim().to_string(), AttrOp::Exists, String::new())
}

fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"'))
            || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len()-1].to_string()
    } else {
        s.to_string()
    }
}

// ─── CSS Property Application ─────────────────────────────────────────────────

/// Walk the box tree and apply `animation_overrides` (from `Document::tick_animations`)
/// on top of the cascaded computed styles.
pub fn apply_animation_overrides(
    node:      &mut HtmlBox,
    overrides: &HashMap<u32, Vec<(String, String)>>,
) {
    let id = node.node_id;
    if let Some(props) = overrides.get(&id) {
        for (prop, val) in props {
            apply_property(&mut node.style, prop, val);
        }
        // Propagate inherited properties (like `color`) to descendant text nodes.
        // Text nodes have no explicit CSS rules — they inherit everything — so their
        // cloned style must track the parent's animated values.  Without this, runs
        // built by collect_items carry the stale pre-animation cascade color.
        let inherited: Vec<(&str, &str)> = props.iter()
            .filter(|(p, _)| is_inherited_css_prop(p))
            .map(|(p, v)| (p.as_str(), v.as_str()))
            .collect();
        if !inherited.is_empty() {
            propagate_to_text_descendants(&mut node.children, &inherited);
        }
    }
    for child in &mut node.children {
        apply_animation_overrides(child, overrides);
    }
}

/// CSS properties that are inherited and must be propagated to text-node descendants.
fn is_inherited_css_prop(prop: &str) -> bool {
    matches!(prop,
        "color" | "font-size" | "font-weight" | "font-style" | "font-family" |
        "letter-spacing" | "word-spacing" | "line-height" | "text-transform" |
        "text-indent" | "visibility" | "cursor"
    )
}

/// Walk `children` recursively and apply `props` to any text-node (`#text`) descendants.
/// Non-text nodes are recursed into so that nested inline elements (e.g. `<span><em>`)
/// also have their text-node leaves updated.  Nodes that have their own entry in
/// `animation_overrides` will be handled separately; here we only touch text nodes,
/// which never have transitions of their own.
fn propagate_to_text_descendants(children: &mut Vec<HtmlBox>, props: &[(&str, &str)]) {
    for child in children {
        if child.is_text_node() {
            for &(prop, val) in props {
                apply_property(&mut child.style, prop, val);
            }
        } else {
            propagate_to_text_descendants(&mut child.children, props);
        }
    }
}

/// Apply a single CSS property/value pair to a ComputedStyle.
/// Copy a single CSS property from parent's computed style into `style`.
/// Used when a rule declares `property: inherit` and must override a lower-specificity
/// concrete value that already changed the inherited default.
fn copy_property_from_parent(style: &mut ComputedStyle, parent: &ComputedStyle, prop: &str) {
    match prop {
        "font-size"       => style.font_size       = parent.font_size.clone(),
        "font-weight"     => style.font_weight     = parent.font_weight,
        "font-family"     => style.font_family     = parent.font_family.clone(),
        "font-style"      => style.font_style      = parent.font_style,
        "color"           => style.color            = parent.color,
        "line-height"     => style.line_height      = parent.line_height.clone(),
        "text-align"      => style.text_align       = parent.text_align,
        "text-decoration" => style.text_decoration  = parent.text_decoration,
        "letter-spacing"  => style.letter_spacing   = parent.letter_spacing.clone(),
        "word-spacing"    => style.word_spacing     = parent.word_spacing.clone(),
        "white-space"     => style.white_space      = parent.white_space,
        "text-transform"  => style.text_transform   = parent.text_transform,
        "direction"       => style.direction        = parent.direction,
        "visibility"      => style.visibility       = parent.visibility,
        "cursor"          => style.cursor           = parent.cursor,
        "display"         => style.display          = parent.display,
        "width"           => style.width            = parent.width.clone(),
        "height"          => style.height           = parent.height.clone(),
        "margin-top"      => style.margin_top       = parent.margin_top.clone(),
        "margin-right"    => style.margin_right     = parent.margin_right.clone(),
        "margin-bottom"   => style.margin_bottom    = parent.margin_bottom.clone(),
        "margin-left"     => style.margin_left      = parent.margin_left.clone(),
        "padding-top"     => style.padding_top      = parent.padding_top.clone(),
        "padding-right"   => style.padding_right    = parent.padding_right.clone(),
        "padding-bottom"  => style.padding_bottom   = parent.padding_bottom.clone(),
        "padding-left"    => style.padding_left     = parent.padding_left.clone(),
        "border-top-width"    => style.border_top_width    = parent.border_top_width.clone(),
        "border-right-width"  => style.border_right_width  = parent.border_right_width.clone(),
        "border-bottom-width" => style.border_bottom_width = parent.border_bottom_width.clone(),
        "border-left-width"   => style.border_left_width   = parent.border_left_width.clone(),
        "border-top-color"    => style.border_top_color    = parent.border_top_color,
        "border-right-color"  => style.border_right_color  = parent.border_right_color,
        "border-bottom-color" => style.border_bottom_color = parent.border_bottom_color,
        "border-left-color"   => style.border_left_color   = parent.border_left_color,
        "border" => {
            style.border_top_width    = parent.border_top_width.clone();
            style.border_right_width  = parent.border_right_width.clone();
            style.border_bottom_width = parent.border_bottom_width.clone();
            style.border_left_width   = parent.border_left_width.clone();
            style.border_top_color    = parent.border_top_color;
            style.border_right_color  = parent.border_right_color;
            style.border_bottom_color = parent.border_bottom_color;
            style.border_left_color   = parent.border_left_color;
            style.border_top_style    = parent.border_top_style;
            style.border_right_style  = parent.border_right_style;
            style.border_bottom_style = parent.border_bottom_style;
            style.border_left_style   = parent.border_left_style;
        }
        "margin" => {
            style.margin_top    = parent.margin_top.clone();
            style.margin_right  = parent.margin_right.clone();
            style.margin_bottom = parent.margin_bottom.clone();
            style.margin_left   = parent.margin_left.clone();
        }
        "padding" => {
            style.padding_top    = parent.padding_top.clone();
            style.padding_right  = parent.padding_right.clone();
            style.padding_bottom = parent.padding_bottom.clone();
            style.padding_left   = parent.padding_left.clone();
        }
        "background-color" => style.background_color = parent.background_color,
        "background"       => style.background_color = parent.background_color,
        "opacity"          => style.opacity     = parent.opacity,
        "overflow"         => { style.overflow_x = parent.overflow_x; style.overflow_y = parent.overflow_y; }
        "overflow-x"       => style.overflow_x  = parent.overflow_x,
        "overflow-y"       => style.overflow_y  = parent.overflow_y,
        "position"         => style.position    = parent.position,
        "float"            => style.float       = parent.float,
        "text-indent"      => style.text_indent = parent.text_indent.clone(),
        "list-style-type"  => style.list_style_type = parent.list_style_type,
        "vertical-align"   => style.vertical_align  = parent.vertical_align,
        "text-overflow"    => style.text_overflow    = parent.text_overflow,
        "word-break"       => style.word_break       = parent.word_break,
        "overflow-wrap"    => style.overflow_wrap     = parent.overflow_wrap,
        "font-stretch"     => style.font_stretch     = parent.font_stretch,
        "text-shadow"      => style.text_shadow      = parent.text_shadow.clone(),
        _ => {} // Unhandled properties — no-op
    }
}

pub fn apply_property(style: &mut ComputedStyle, prop: &str, value: &str) {
    // HTML attributes that aren't real CSS properties — handle before resolving
    match prop {
        "cellpadding" => {
            let v = value.trim();
            style.cell_padding = parse_length(v);
            return;
        }
        "cellspacing" => {
            let v = value.trim();
            style.border_spacing_h = parse_length(v);
            style.border_spacing_v = parse_length(v);
            return;
        }
        _ => {}
    }
    // CSS custom properties (--*) are not resolved by PropertyId
    let v = value.trim();
    if prop.starts_with("--") {
        style.custom_props.insert(prop.to_string(), v.to_string());
        return;
    }
    let id = properties::resolve(prop);
    apply_property_by_id(style, id, value);
}

pub fn apply_property_by_id(style: &mut ComputedStyle, id: properties::PropertyId, value: &str) {
    let v = value.trim();
    // `inherit` means "use the parent's computed value". For inherited properties,
    // `inherit_from` already copied the parent value before rules are applied, so
    // skipping the application keeps the inherited value intact. For non-inherited
    // properties this is also the safest default (avoids incorrect fallback).
    if v == "inherit" { return; }
    // `initial` / `revert` / `unset` — treat as reset to default (skip for now, close enough).
    if matches!(v, "initial" | "revert" | "unset" | "revert-layer") { return; }
    (property_defs::get(id).apply)(style, v);
}

/// Parse a CSS `transform` value string into a `CssTransform`.
pub fn parse_css_transform(v: &str) -> crate::types::CssTransform {
    use crate::types::{CssTransform, TransformOp};
    let mut ops = Vec::new();
    let v = v.trim();
    if v == "none" { return CssTransform::default(); }
    // Simple tokenizer: split on function calls like "translate(10px, 20px) rotate(45deg)"
    let mut rest = v;
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() { break; }
        // Find function name up to '('
        let paren_pos = match rest.find('(') {
            Some(p) => p,
            None    => break,
        };
        let func = rest[..paren_pos].trim().to_ascii_lowercase();
        let after_paren = &rest[paren_pos + 1..];
        // Find matching closing paren
        let close = after_paren.find(')').unwrap_or(after_paren.len());
        let args_str = &after_paren[..close];
        rest = if close + 1 < after_paren.len() { &after_paren[close + 1..] } else { "" };

        // Parse comma/whitespace-separated float args
        let args: Vec<f32> = args_str.split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .map(|s| {
                let s = s.trim().trim_end_matches("px").trim_end_matches("deg")
                          .trim_end_matches("rad").trim_end_matches("turn");
                s.parse::<f32>().unwrap_or(0.0)
            })
            .collect();

        let get = |i: usize, def: f32| -> f32 { *args.get(i).unwrap_or(&def) };

        match func.as_str() {
            "translate"  => ops.push(TransformOp::Translate(get(0, 0.0), get(1, 0.0))),
            "translatex" => ops.push(TransformOp::TranslateX(get(0, 0.0))),
            "translatey" => ops.push(TransformOp::TranslateY(get(0, 0.0))),
            "scale"      => ops.push(TransformOp::Scale(get(0, 1.0), get(1, get(0, 1.0)))),
            "scalex"     => ops.push(TransformOp::ScaleX(get(0, 1.0))),
            "scaley"     => ops.push(TransformOp::ScaleY(get(0, 1.0))),
            "rotate"     => ops.push(TransformOp::Rotate(get(0, 0.0))),
            "skewx"      => ops.push(TransformOp::SkewX(get(0, 0.0))),
            "skewy"      => ops.push(TransformOp::SkewY(get(0, 0.0))),
            "matrix"     => ops.push(TransformOp::Matrix(
                get(0, 1.0), get(1, 0.0), get(2, 0.0),
                get(3, 1.0), get(4, 0.0), get(5, 0.0),
            )),
            _ => {}
        }
    }
    CssTransform { ops }
}

/// Parse a CSS `transform-origin` value into (x, y) fractions (0.0..1.0).
pub fn parse_transform_origin(v: &str) -> (f32, f32) {
    let parts: Vec<&str> = v.split_whitespace().collect();
    let parse_one = |s: &str| -> f32 {
        match s {
            "left"   | "top"    => 0.0,
            "center"            => 0.5,
            "right"  | "bottom" => 1.0,
            _ if s.ends_with('%')  => s[..s.len()-1].parse::<f32>().unwrap_or(50.0) / 100.0,
            _ if s.ends_with("px") => s[..s.len()-2].parse::<f32>().unwrap_or(0.0),
            _ => s.parse::<f32>().unwrap_or(0.5),
        }
    };
    let x = parts.first().map(|s| parse_one(s)).unwrap_or(0.5);
    let y = parts.get(1).map(|s| parse_one(s)).unwrap_or(0.5);
    (x, y)
}

/// Parse a CSS `filter` value string into `CssFilters`.
pub fn parse_css_filter(v: &str) -> crate::types::CssFilters {
    use crate::types::{CssFilters, FilterOp};
    let mut ops = Vec::new();
    if v.trim() == "none" { return CssFilters::default(); }
    let mut rest = v.trim();
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() { break; }
        let paren_pos = match rest.find('(') { Some(p) => p, None => break };
        let func = rest[..paren_pos].trim().to_ascii_lowercase();
        let after_paren = &rest[paren_pos + 1..];
        let close = after_paren.find(')').unwrap_or(after_paren.len());
        let arg_str = after_paren[..close].trim();
        rest = if close + 1 < after_paren.len() { &after_paren[close + 1..] } else { "" };
        // Parse arg as f32 (strip %, px, deg)
        let arg: f32 = arg_str.trim_end_matches('%').trim_end_matches("px")
                               .trim_end_matches("deg").parse().unwrap_or(0.0);
        // Normalize percent-based args to 0..1
        let arg_norm = if arg_str.ends_with('%') { arg / 100.0 } else { arg };
        match func.as_str() {
            "blur"        => ops.push(FilterOp::Blur(arg)),
            "brightness"  => ops.push(FilterOp::Brightness(arg_norm)),
            "contrast"    => ops.push(FilterOp::Contrast(arg_norm)),
            "grayscale"   => ops.push(FilterOp::Grayscale(arg_norm)),
            "hue-rotate"  => ops.push(FilterOp::HueRotate(arg)),
            "invert"      => ops.push(FilterOp::Invert(arg_norm)),
            "opacity"     => ops.push(FilterOp::Opacity(arg_norm)),
            "saturate"    => ops.push(FilterOp::Saturate(arg_norm)),
            "sepia"       => ops.push(FilterOp::Sepia(arg_norm)),
            "drop-shadow" => {
                // drop-shadow(dx dy blur color)
                let parts: Vec<&str> = arg_str.split_whitespace().collect();
                let pf = |i: usize| parts.get(i)
                    .and_then(|s| s.trim_end_matches("px").parse::<f32>().ok())
                    .unwrap_or(0.0);
                ops.push(FilterOp::DropShadow {
                    dx: pf(0), dy: pf(1), blur: pf(2),
                    color: parts.get(3).and_then(|s| parse_color(s))
                        .unwrap_or(crate::types::Color::BLACK),
                });
            }
            _ => {}
        }
    }
    CssFilters { ops }
}

/// Resolve `var(--name)` and `var(--name, fallback)` references in a CSS value.
/// Variables in the map are pre-resolved by `pre_resolve_variables`, so one pass suffices.
/// Any still-unresolved var() (unknown custom property with no fallback) is dropped.
pub fn resolve_var_references(val: &str, variables: &HashMap<String, String>) -> String {
    if !val.contains("var(") { return val.to_string(); }
    let resolved = resolve_var_pass(val, variables);
    // Drop any remaining unresolved var() by substituting with fallback or "".
    if resolved.contains("var(") { resolve_var_pass(&resolved, &HashMap::new()) } else { resolved }
}

fn resolve_var_pass(val: &str, variables: &HashMap<String, String>) -> String {
    if !val.contains("var(") {
        return val.to_string();
    }
    let mut out = String::new();
    let mut rest = val;
    while !rest.is_empty() {
        if let Some(start) = rest.find("var(") {
            out.push_str(&rest[..start]);
            rest = &rest[start + 4..]; // skip "var("
            // find matching closing paren
            let mut depth = 1usize;
            let mut end = 0;
            let bytes = rest.as_bytes();
            while end < bytes.len() {
                match bytes[end] {
                    b'(' => depth += 1,
                    b')' => { depth -= 1; if depth == 0 { break; } }
                    _ => {}
                }
                end += 1;
            }
            let inner = &rest[..end];
            rest = if end < rest.len() { &rest[end+1..] } else { "" };
            // inner = "--name" or "--name, fallback"
            let (name, fallback) = if let Some(comma) = inner.find(',') {
                (inner[..comma].trim(), Some(inner[comma+1..].trim()))
            } else {
                (inner.trim(), None)
            };
            if let Some(resolved) = variables.get(name) {
                if resolved.is_empty() {
                    if let Some(fb) = fallback {
                        out.push_str(fb);
                    }
                } else {
                    out.push_str(resolved);
                }
            } else if let Some(fb) = fallback {
                out.push_str(fb);
            }
        } else {
            out.push_str(rest);
            break;
        }
    }
    out
}

/// Resolve a CSS `content` property value string to a displayable string.
/// Handles string literals, open-quote/close-quote, and discards complex expressions.
/// Process CSS Unicode escapes in a string: `\e001` → U+E001, `\A` → newline, etc.
fn unescape_css_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            // Collect up to 6 hex digits
            let mut hex = String::new();
            while hex.len() < 6 {
                match chars.peek() {
                    Some(ch) if ch.is_ascii_hexdigit() => { hex.push(*ch); chars.next(); }
                    _ => break,
                }
            }
            if !hex.is_empty() {
                // Optional trailing whitespace is consumed after hex escape
                if let Some(&' ') = chars.peek() { chars.next(); }
                if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                    if let Some(uc) = char::from_u32(cp) {
                        out.push(uc);
                        continue;
                    }
                }
                // Invalid code point — output replacement character
                out.push('\u{FFFD}');
            } else if let Some(next) = chars.next() {
                // Escaped literal character (e.g. \\ → \, \" → ")
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn resolve_content_value(v: &str) -> String {
    let v = v.trim();
    match v {
        "none" | "normal" => String::new(),
        "open-quote"      => "\u{201C}".to_string(),  // "
        "close-quote"     => "\u{201D}".to_string(),  // "
        "no-open-quote" | "no-close-quote" => String::new(),
        _ => {
            // Quoted string: "text" or 'text'
            if (v.starts_with('"') && v.ends_with('"'))
                || (v.starts_with('\'') && v.ends_with('\''))
            {
                return unescape_css_string(&v[1..v.len()-1]);
            }
            // Multiple tokens (e.g. '"foo" open-quote'): concatenate resolved parts
            let mut out = String::new();
            let mut rest = v;
            while !rest.is_empty() {
                rest = rest.trim_start();
                if rest.starts_with('"') || rest.starts_with('\'') {
                    let q = &rest[..1];
                    if let Some(end) = rest[1..].find(q) {
                        out.push_str(&unescape_css_string(&rest[1..end+1]));
                        rest = &rest[end+2..];
                    } else {
                        out.push_str(&unescape_css_string(&rest[1..]));
                        break;
                    }
                } else {
                    // keyword token
                    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
                    let tok = &rest[..end];
                    match tok {
                        "open-quote"    => out.push('\u{201C}'),
                        "close-quote"   => out.push('\u{201D}'),
                        "no-open-quote" | "no-close-quote" => {}
                        _ => {
                            // counter(name) → emit placeholder for later resolution
                            if tok.starts_with("counter(") && tok.ends_with(')') {
                                out.push('\x01');
                                out.push_str(tok);
                                out.push('\x01');
                            }
                            // attr(), counters(), etc. — ignore for now
                        }
                    }
                    rest = &rest[end..];
                }
            }
            out
        }
    }
}

/// Resolve counter() and counters() function calls in ::before/::after content.
fn resolve_counters_in_content(content: &str, counters: &HashMap<String, Vec<i32>>) -> String {
    if !content.contains('\x01') { return content.to_string(); }
    // Placeholder \x01counter(name)\x01 was inserted by resolve_content_value
    let mut out = String::new();
    let mut rest = content;
    while let Some(start) = rest.find('\x01') {
        out.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        if let Some(end) = rest.find('\x01') {
            let func = &rest[..end];
            rest = &rest[end + 1..];
            if let Some(inner) = func.strip_prefix("counter(").and_then(|s| s.strip_suffix(')')) {
                let name = inner.split(',').next().unwrap_or("").trim();
                let val = counters.get(name).and_then(|s| s.last()).copied().unwrap_or(0);
                out.push_str(&val.to_string());
            } else {
                out.push_str(func);
            }
        }
    }
    out.push_str(rest);
    out
}

/// Parse CSS counter list: "name1 3 name2 name3 -1" → [(name1,3),(name2,1),(name3,-1)]
pub fn parse_counter_list(v: &str) -> Vec<(String, i32)> {
    if v == "none" { return Vec::new(); }
    let mut result = Vec::new();
    let toks: Vec<&str> = v.split_whitespace().collect();
    let mut i = 0;
    while i < toks.len() {
        let name = toks[i].to_string();
        i += 1;
        let val = if i < toks.len() {
            if let Ok(n) = toks[i].parse::<i32>() { i += 1; n } else { 1 }
        } else { 1 };
        result.push((name, val));
    }
    result
}

/// Split a shorthand value into top-level tokens, respecting parentheses.
/// E.g. "rgb(200,200,200) red" → ["rgb(200,200,200)", "red"]
pub fn split_shorthand_values(v: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    for (i, ch) in v.char_indices() {
        match ch {
            '(' => { depth += 1; if start.is_none() { start = Some(i); } }
            ')' => { depth = depth.saturating_sub(1); }
            _ if ch.is_whitespace() && depth == 0 => {
                if let Some(s) = start {
                    parts.push(&v[s..i]);
                    start = None;
                }
            }
            _ => { if start.is_none() { start = Some(i); } }
        }
    }
    if let Some(s) = start {
        parts.push(&v[s..]);
    }
    parts
}

pub fn apply_shorthand_4<F: Fn(&str) -> CssLength>(
    v: &str,
    top: &mut CssLength, right: &mut CssLength,
    bottom: &mut CssLength, left: &mut CssLength,
    parse: F,
) {
    let parts: Vec<&str> = v.split_whitespace().collect();
    match parts.len() {
        1 => { let x = parse(parts[0]); *top = x.clone(); *right = x.clone(); *bottom = x.clone(); *left = x; }
        2 => { let tb = parse(parts[0]); let rl = parse(parts[1]); *top = tb.clone(); *bottom = tb; *right = rl.clone(); *left = rl; }
        3 => { *top = parse(parts[0]); let rl = parse(parts[1]); *right = rl.clone(); *left = rl; *bottom = parse(parts[2]); }
        4 => { *top = parse(parts[0]); *right = parse(parts[1]); *bottom = parse(parts[2]); *left = parse(parts[3]); }
        _ => {}
    }
}

pub fn apply_border_shorthand(style: &mut ComputedStyle, v: &str) {
    // border: <width> <style> <color>
    for part in v.split_whitespace() {
        if let Some(bs) = try_parse_border_style(part) {
            style.border_top_style    = bs;
            style.border_right_style  = bs;
            style.border_bottom_style = bs;
            style.border_left_style   = bs;
        } else if let Some(c) = parse_color(part) {
            style.border_top_color    = c;
            style.border_right_color  = c;
            style.border_bottom_color = c;
            style.border_left_color   = c;
        } else {
            let w = parse_length(part);
            if !matches!(w, CssLength::Auto) {
                style.border_top_width    = w.clone();
                style.border_right_width  = w.clone();
                style.border_bottom_width = w.clone();
                style.border_left_width   = w;
            }
        }
    }
}

pub fn apply_border_side_shorthand(
    v:     &str,
    width: &mut CssLength,
    style: &mut BorderStyle,
    color: &mut Color,
) {
    for part in v.split_whitespace() {
        if let Some(bs) = try_parse_border_style(part) {
            *style = bs;
        } else if let Some(c) = parse_color(part) {
            *color = c;
        } else {
            let w = parse_length(part);
            if !matches!(w, CssLength::Auto) {
                *width = w;
            }
        }
    }
}

pub fn extract_url(v: &str) -> Option<String> {
    let lower = v.to_lowercase();
    let start = lower.find("url(")?;
    let inner = v[start + 4..].trim();
    let inner = inner.trim_start_matches('"').trim_start_matches('\'');
    let end = inner.find(|c| c == ')' || c == '"' || c == '\'')?;
    Some(inner[..end].to_string())
}

/// Resolve all `url()` references in CSS text relative to the CSS file's URL.
/// This ensures that `url('../image.jpg')` in an external stylesheet is resolved
/// relative to the stylesheet, not the HTML document.
pub fn resolve_css_urls(css: &str, css_base_url: &str) -> String {
    if css_base_url.is_empty() || !css_base_url.contains("://") {
        return css.to_string();
    }
    // Find the directory of the CSS file URL
    let css_dir = if let Some(last_slash) = css_base_url.rfind('/') {
        &css_base_url[..=last_slash]
    } else {
        css_base_url
    };

    let mut result = String::with_capacity(css.len());
    let mut remaining = css;
    while let Some(url_start) = remaining.to_lowercase().find("url(") {
        // Copy everything before url(
        result.push_str(&remaining[..url_start]);
        let after_url = &remaining[url_start + 4..];
        let inner = after_url.trim_start();
        // Find the closing )
        let mut depth = 1;
        let mut end_idx = 0;
        for (i, ch) in after_url.char_indices() {
            if ch == '(' { depth += 1; }
            if ch == ')' { depth -= 1; if depth == 0 { end_idx = i; break; } }
        }
        if end_idx == 0 {
            // Malformed — just copy as-is
            result.push_str(&remaining[url_start..]);
            break;
        }
        let url_content = after_url[..end_idx].trim();
        let url_content = url_content.trim_matches('"').trim_matches('\'');

        // Only resolve relative URLs (not absolute, data:, or already-resolved)
        let resolved = if url_content.contains("://") || url_content.starts_with("data:") || url_content.starts_with('/') {
            url_content.to_string()
        } else {
            format!("{}{}", css_dir, url_content)
        };

        result.push_str(&format!("url('{}')", resolved));
        remaining = &after_url[end_idx + 1..];
    }
    result.push_str(remaining);
    result
}

/// Parse a CSS shadow value: `offset_x offset_y [blur] [color]`
/// Returns (offset_x, offset_y, blur_radius, color).
pub fn parse_shadow_value(v: &str) -> (f32, f32, f32, Color) {
    let mut nums: Vec<f32> = Vec::new();
    let mut color = Color { r: 0, g: 0, b: 0, a: 255 };
    // Use paren-aware splitting so rgba(r, g, b, a) stays as one token
    let tokens = split_paren_aware(v);
    for tok in &tokens {
        let t = tok.trim();
        if t.is_empty() { continue; }
        if let Some(c) = parse_color(t) {
            color = c;
        } else if let Ok(n) = t.trim_end_matches("px").parse::<f32>() {
            nums.push(n);
        }
    }
    let ox   = nums.first().copied().unwrap_or(0.0);
    let oy   = nums.get(1).copied().unwrap_or(0.0);
    let blur = nums.get(2).copied().unwrap_or(0.0);
    (ox, oy, blur, color)
}

/// Split a string on whitespace, but keep parenthesized groups together.
/// e.g. "2px 2px rgba(0, 0, 0, 0.5)" → ["2px", "2px", "rgba(0, 0, 0, 0.5)"]
fn split_paren_aware(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '(' => { depth += 1; current.push(c); }
            ')' => { if depth > 0 { depth -= 1; } current.push(c); }
            ' ' | '\t' if depth == 0 => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => { current.push(c); }
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Find the byte index of a space that is not nested inside parentheses.
pub fn find_split_space(v: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in v.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => { if depth > 0 { depth -= 1; } }
            ' ' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

pub fn apply_gradient(style: &mut ComputedStyle, v: &str) {
    let lower = v.to_lowercase();
    if lower.contains("linear-gradient") {
        style.gradient_type = GradientType::Linear;
        // Parse angle from "to bottom" or degrees
        if let Some(paren) = v.find('(') {
            let inner = &v[paren + 1..];
            let inner = inner.trim_end_matches(')');
            let first_comma = inner.find(',').unwrap_or(inner.len());
            let dir = inner[..first_comma].trim();
            if dir.ends_with("deg") {
                style.gradient_angle = dir[..dir.len()-3].parse().unwrap_or(180.0);
            } else if dir == "to bottom" || dir == "to top" {
                style.gradient_angle = if dir == "to bottom" { 180.0 } else { 0.0 };
            } else if dir == "to right" {
                style.gradient_angle = 90.0;
            } else if dir == "to left" {
                style.gradient_angle = 270.0;
            }
            // Parse color stops
            let stops_str = if first_comma < inner.len() { &inner[first_comma + 1..] } else { "" };
            style.gradient_stops.clear();
            let n_stops = stops_str.split(',').count().max(1) as f32;
            for (i, stop) in stops_str.split(',').enumerate() {
                let stop = stop.trim();
                // Each stop may be "color position%"
                let mut parts = stop.splitn(2, ' ');
                let color_str = parts.next().unwrap_or(stop);
                if let Some(c) = parse_color(color_str) {
                    let pos = parts.next()
                        .and_then(|p| p.trim_end_matches('%').parse::<f32>().ok())
                        .map(|p| p / 100.0)
                        .unwrap_or(i as f32 / (n_stops - 1.0).max(1.0));
                    style.gradient_stops.push(GradientStop { color: c, position: pos });
                }
            }
        }
    } else if lower.contains("radial-gradient") {
        style.gradient_type = GradientType::Radial;
        if let Some(paren) = v.find('(') {
            let inner = &v[paren + 1..];
            let inner = inner.trim_end_matches(')');
            style.gradient_stops.clear();
            // Skip the optional shape/size/position descriptor before the first comma
            // (e.g. "circle at 50% 50%", "ellipse farthest-corner", "closest-side").
            // If the first comma-delimited segment doesn't parse as a color, treat it as a descriptor.
            let first_comma = inner.find(',').unwrap_or(inner.len());
            let first_token = inner[..first_comma].trim();
            let stops_str = if parse_color(first_token).is_none() && first_comma < inner.len() {
                &inner[first_comma + 1..]
            } else {
                inner
            };
            let stops: Vec<&str> = stops_str.split(',').collect();
            let n_stops = stops.len() as f32;
            for (i, stop) in stops.iter().enumerate() {
                let stop = stop.trim();
                // Each stop is "color" or "color position%"
                let mut parts = stop.splitn(2, ' ');
                let color_str = parts.next().unwrap_or(stop);
                if let Some(c) = parse_color(color_str) {
                    let pos = parts.next()
                        .and_then(|p| p.trim().trim_end_matches('%').parse::<f32>().ok())
                        .map(|p| p / 100.0)
                        .unwrap_or(i as f32 / (n_stops - 1.0).max(1.0));
                    style.gradient_stops.push(GradientStop { color: c, position: pos });
                }
            }
        }
    }
}

/// Return true if a token looks like a font-size value (keyword or length unit).
fn is_font_size_token(tok: &str) -> bool {
    matches!(tok, "xx-small"|"x-small"|"small"|"medium"|"large"|"x-large"|"xx-large"|"smaller"|"larger")
    || tok.ends_with("px") || tok.ends_with("em") || tok.ends_with("rem")
    || tok.ends_with('%') || tok.ends_with("pt") || tok.ends_with("vw") || tok.ends_with("vh")
}

pub fn apply_font_shorthand(style: &mut ComputedStyle, v: &str) {
    // CSS font shorthand: [style] [variant] [weight] [stretch] size[/line-height] family-list
    // System font keywords (single-token):
    if matches!(v, "caption"|"icon"|"menu"|"message-box"|"small-caption"|"status-bar") {
        return; // Use UA defaults; no overrides.
    }

    // Tokenise the shorthand, respecting quoted strings in the family part.
    // Split by whitespace for the pre-family part; the family is everything after size.
    let v = v.trim();
    let mut size_found_at: Option<usize> = None; // byte offset in v
    let mut byte_pos = 0usize;

    // Walk tokens to find the font-size token (first length/keyword that can be a size).
    for tok in v.split_whitespace() {
        // Handle "size/line-height" as a single token.
        let size_tok = if tok.contains('/') {
            tok.splitn(2, '/').next().unwrap_or(tok)
        } else {
            tok
        };

        if is_font_size_token(size_tok) {
            // Parse size (and optional /line-height).
            if tok.contains('/') {
                let mut parts = tok.splitn(2, '/');
                style.font_size = parse_font_size(parts.next().unwrap_or(""));
                if let Some(lh) = parts.next() { style.line_height = parse_line_height(lh); }
            } else {
                style.font_size = parse_font_size(tok);
            }
            size_found_at = Some(byte_pos + tok.len());
            break;
        }

        // Pre-size tokens: style / variant / weight.
        match tok {
            "italic"  | "oblique"   => {
                style.font_style = if tok == "italic" { FontStyle::Italic } else { FontStyle::Oblique };
            }
            "small-caps"            => { style.small_caps = true; }
            "bold"                  => { style.font_weight = FontWeight::Bold; }
            "bolder"                => { style.font_weight = FontWeight::Value(700); }
            "lighter"               => { style.font_weight = FontWeight::Value(300); }
            "normal"                => {}
            s if s.parse::<u16>().is_ok() => {
                style.font_weight = FontWeight::Value(s.parse().unwrap());
            }
            _ => {}
        }
        byte_pos += tok.len() + 1; // +1 for the space
    }

    // Everything after the size (and optional /lh) token is the font-family list.
    if let Some(after) = size_found_at {
        let family_part = v[after..].trim();
        // Strip any leading /line-height that wasn't part of the size token.
        let family_part = if family_part.starts_with('/') {
            let rest = family_part[1..].trim();
            // The line-height value is the next whitespace-separated token.
            let mut it = rest.splitn(2, char::is_whitespace);
            let lh_tok = it.next().unwrap_or("");
            style.line_height = parse_line_height(lh_tok);
            it.next().unwrap_or("").trim()
        } else {
            family_part
        };
        if !family_part.is_empty() {
            // Normalize system-font keywords in the family list.
            let normalized = split_font_families(family_part)
                .into_iter()
                .map(|name| {
                    resolve_system_font_keyword(&name)
                        .map(|s| s.to_string())
                        .unwrap_or(name)
                })
                .collect::<Vec<_>>()
                .join(", ");
            style.font_family = normalized;
        }
    }
}

// ─── Value Parsers ────────────────────────────────────────────────────────────

/// Cache for parsed CSS length values — avoids re-parsing the same string
/// (e.g. "100%" or "calc(100% - 21.5rem)") thousands of times during cascade.
static LENGTH_CACHE: std::sync::LazyLock<std::sync::Mutex<HashMap<String, CssLength>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

pub fn parse_length(v: &str) -> CssLength {
    let v = v.trim();
    if v == "auto"       { return CssLength::Auto; }
    if v == "0"          { return CssLength::Zero; }
    // Check cache for previously parsed values
    if let Ok(cache) = LENGTH_CACHE.lock() {
        if let Some(cached) = cache.get(v) {
            return cached.clone();
        }
    }
    let result = parse_length_inner(v);
    // Cache the result (only for non-trivial values)
    if v.len() > 4 {
        if let Ok(mut cache) = LENGTH_CACHE.lock() {
            if cache.len() < 50000 {
                cache.insert(v.to_string(), result.clone());
            }
        }
    }
    result
}

fn parse_length_inner(v: &str) -> CssLength {
    if let Some(inner) = v.strip_prefix("calc(").and_then(|s| s.strip_suffix(')')) {
        return parse_calc(inner);
    }
    // CSS min()/max()/clamp() — proper AST with lazy resolution at layout time
    if let Some(inner) = v.strip_prefix("min(").and_then(|s| s.strip_suffix(')')) {
        let args = split_top_level_commas(inner);
        if args.len() >= 2 {
            let vals: Vec<CssLength> = args.iter().map(|a| parse_length(a.trim())).collect();
            return CssLength::Min(vals.into_boxed_slice());
        }
        return parse_length(inner);
    }
    if let Some(inner) = v.strip_prefix("max(").and_then(|s| s.strip_suffix(')')) {
        let args = split_top_level_commas(inner);
        if args.len() >= 2 {
            let vals: Vec<CssLength> = args.iter().map(|a| parse_length(a.trim())).collect();
            return CssLength::Max(vals.into_boxed_slice());
        }
        return parse_length(inner);
    }
    if let Some(inner) = v.strip_prefix("clamp(").and_then(|s| s.strip_suffix(')')) {
        let args = split_top_level_commas(inner);
        if args.len() == 3 {
            let min = parse_length(args[0].trim());
            let val = parse_length(args[1].trim());
            let max = parse_length(args[2].trim());
            return CssLength::Clamp(Box::new(min), Box::new(val), Box::new(max));
        }
        // Fallback: treat as calc
        return parse_length(inner);
    }
    if v.ends_with("px") { return CssLength::Px(v[..v.len()-2].parse().unwrap_or(0.0)); }
    if v.ends_with("rem") { return CssLength::Rem(v[..v.len()-3].parse().unwrap_or(0.0)); }
    if v.ends_with("em") { return CssLength::Em(v[..v.len()-2].parse().unwrap_or(0.0)); }
    if v.ends_with('%')  { return CssLength::Percent(v[..v.len()-1].parse().unwrap_or(0.0)); }
    if v.ends_with("pt") { return CssLength::Px(v[..v.len()-2].parse::<f32>().unwrap_or(0.0) * 4.0 / 3.0); }
    if v.ends_with("vw") { return CssLength::Vw(v[..v.len()-2].parse().unwrap_or(0.0)); }
    if v.ends_with("vh") { return CssLength::Vh(v[..v.len()-2].parse().unwrap_or(0.0)); }
    if v.ends_with("vmin") { let n = v[..v.len()-4].parse::<f32>().unwrap_or(0.0); return CssLength::Vw(n); } // approx
    if v.ends_with("vmax") { let n = v[..v.len()-4].parse::<f32>().unwrap_or(0.0); return CssLength::Vw(n); } // approx
    // Unitless number (treat as px for simplicity)
    if let Ok(n) = v.parse::<f32>() { return CssLength::Px(n); }
    CssLength::Auto
}

/// Parse the inside of `calc(...)` using recursive descent.
///
/// Handles arbitrary nesting, mixed units, and correct operator precedence:
///   calc(100% - 21.5rem + (100vw - 1569px) / 2)
///
/// The result is a linear combination of unit coefficients [pct, px, em, rem, vw, vh].
/// At layout time, each coefficient is multiplied by its resolved unit value.
fn parse_calc(expr: &str) -> CssLength {
    let expr = expr.trim();
    // If the expression contains min()/max()/clamp(), use tree-based parser
    // since these can't be represented as linear coefficients.
    if expr.contains("min(") || expr.contains("max(") || expr.contains("clamp(") {
        let node = parse_calc_tree(expr);
        return CssLength::CalcExpr(Box::new(node));
    }
    let bytes = expr.as_bytes();
    let mut pos = 0usize;
    let coeffs = calc_parse_additive(bytes, &mut pos);
    let vals = coeffs;
    // Simplify: if only one unit is non-zero, return a simple CssLength variant.
    let n_nonzero = vals.iter().filter(|&&v| v != 0.0).count();
    if n_nonzero <= 1 {
        if vals[0] != 0.0 { return CssLength::Percent(vals[0]); }
        if vals[2] != 0.0 { return CssLength::Em(vals[2]); }
        if vals[3] != 0.0 { return CssLength::Rem(vals[3]); }
        if vals[4] != 0.0 { return CssLength::Vw(vals[4]); }
        if vals[5] != 0.0 { return CssLength::Vh(vals[5]); }
        return CssLength::Px(vals[1]);
    }
    CssLength::Calc(vals)
}

/// Parse calc() expression into a CalcNode tree (handles nested min/max/clamp).
fn parse_calc_tree(expr: &str) -> CalcNode {
    use crate::types::CalcNode;
    let expr = expr.trim();

    // Split on top-level `+` and `-` (with spaces, per CSS spec)
    let parts = split_calc_additive(expr);
    if parts.len() == 1 {
        return parse_calc_tree_multiplicative(parts[0].1);
    }

    let mut result = parse_calc_tree_multiplicative(parts[0].1);
    for &(sign, term) in &parts[1..] {
        let rhs = parse_calc_tree_multiplicative(term);
        result = if sign == '+' {
            CalcNode::Add(Box::new(result), Box::new(rhs))
        } else {
            CalcNode::Sub(Box::new(result), Box::new(rhs))
        };
    }
    result
}

fn parse_calc_tree_multiplicative(expr: &str) -> CalcNode {
    use crate::types::CalcNode;
    let expr = expr.trim();
    // Simple: check for * or / not inside parens
    let mut depth = 0i32;
    let mut last_op = 0usize;
    let mut op_char = 0u8;
    let bytes = expr.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'*' | b'/' if depth == 0 && i > 0 => {
                last_op = i;
                op_char = b;
            }
            _ => {}
        }
    }
    if op_char != 0 && last_op > 0 {
        let lhs = parse_calc_tree_atom(&expr[..last_op]);
        let rhs_str = expr[last_op + 1..].trim();
        if let Ok(scalar) = rhs_str.parse::<f32>() {
            return if op_char == b'*' {
                CalcNode::Mul(Box::new(lhs), scalar)
            } else {
                CalcNode::Div(Box::new(lhs), scalar)
            };
        }
    }
    parse_calc_tree_atom(expr)
}

fn parse_calc_tree_atom(expr: &str) -> CalcNode {
    use crate::types::CalcNode;
    let expr = expr.trim();
    // Parenthesized
    if expr.starts_with('(') && expr.ends_with(')') {
        return parse_calc_tree(&expr[1..expr.len()-1]);
    }
    // min/max/clamp — delegate to parse_length which handles these
    if expr.starts_with("min(") || expr.starts_with("max(") || expr.starts_with("clamp(") {
        return CalcNode::Value(parse_length(expr));
    }
    // Simple value
    CalcNode::Value(parse_length(expr))
}

/// Split a calc expression at top-level `+` and `-` operators (CSS requires spaces around them).
fn split_calc_additive(expr: &str) -> Vec<(char, &str)> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let bytes = expr.as_bytes();
    let mut start = 0usize;
    let mut sign = '+';
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b' ' if depth == 0 && i + 2 < bytes.len() => {
                let op = bytes[i + 1];
                if (op == b'+' || op == b'-') && bytes[i + 2] == b' ' {
                    let term = expr[start..i].trim();
                    if !term.is_empty() {
                        parts.push((sign, term));
                    }
                    sign = op as char;
                    i += 3;
                    start = i;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let tail = expr[start..].trim();
    if !tail.is_empty() {
        parts.push((sign, tail));
    }
    if parts.is_empty() {
        parts.push(('+', expr));
    }
    parts
}

// ── Recursive descent calc() evaluator ───────────────────────────────────────
// Coefficients: [percent, px, em, rem, vw, vh]
type Coeffs = [f32; 6];
const ZERO_COEFFS: Coeffs = [0.0; 6];

fn coeffs_add(a: &Coeffs, b: &Coeffs) -> Coeffs {
    [a[0]+b[0], a[1]+b[1], a[2]+b[2], a[3]+b[3], a[4]+b[4], a[5]+b[5]]
}
fn coeffs_sub(a: &Coeffs, b: &Coeffs) -> Coeffs {
    [a[0]-b[0], a[1]-b[1], a[2]-b[2], a[3]-b[3], a[4]-b[4], a[5]-b[5]]
}
fn coeffs_mul(a: &Coeffs, f: f32) -> Coeffs {
    [a[0]*f, a[1]*f, a[2]*f, a[3]*f, a[4]*f, a[5]*f]
}

fn calc_skip_ws(b: &[u8], pos: &mut usize) {
    while *pos < b.len() && (b[*pos] == b' ' || b[*pos] == b'\t') { *pos += 1; }
}

/// Additive level: handles `+` and `-` (lowest precedence).
fn calc_parse_additive(b: &[u8], pos: &mut usize) -> Coeffs {
    let mut result = calc_parse_multiplicative(b, pos);
    loop {
        calc_skip_ws(b, pos);
        if *pos >= b.len() { break; }
        // CSS calc requires spaces around + and - operators.
        // Check for ` + ` or ` - ` pattern (we already consumed leading ws).
        let op = b[*pos];
        if (op == b'+' || op == b'-') && *pos + 1 < b.len() && b[*pos + 1] == b' ' {
            // Make sure the previous char was a space (we consumed it in skip_ws)
            *pos += 1; // skip operator
            calc_skip_ws(b, pos);
            let rhs = calc_parse_multiplicative(b, pos);
            result = if op == b'+' { coeffs_add(&result, &rhs) } else { coeffs_sub(&result, &rhs) };
        } else {
            break;
        }
    }
    result
}

/// Multiplicative level: handles `*` and `/` (higher precedence).
fn calc_parse_multiplicative(b: &[u8], pos: &mut usize) -> Coeffs {
    let mut result = calc_parse_atom(b, pos);
    loop {
        calc_skip_ws(b, pos);
        if *pos >= b.len() { break; }
        let op = b[*pos];
        if op == b'*' || op == b'/' {
            *pos += 1;
            calc_skip_ws(b, pos);
            if op == b'*' {
                // One side must be a plain number. Try: coeffs * number or number * coeffs.
                // We already have lhs as coeffs, so rhs should be a number.
                let rhs = calc_parse_atom(b, pos);
                // If rhs is purely px (unitless number parsed as px), use as scalar.
                // If lhs is purely px, treat lhs as scalar and rhs as unit-bearing.
                let rhs_scalar = if rhs[0] == 0.0 && rhs[2] == 0.0 && rhs[3] == 0.0 && rhs[4] == 0.0 && rhs[5] == 0.0 {
                    Some(rhs[1])
                } else { None };
                let lhs_scalar = if result[0] == 0.0 && result[2] == 0.0 && result[3] == 0.0 && result[4] == 0.0 && result[5] == 0.0 {
                    Some(result[1])
                } else { None };
                if let Some(s) = rhs_scalar {
                    result = coeffs_mul(&result, s);
                } else if let Some(s) = lhs_scalar {
                    result = coeffs_mul(&rhs, s);
                } else {
                    // Both have units — invalid in CSS, just keep lhs
                }
            } else {
                // Division: coeffs / number
                let rhs = calc_parse_atom(b, pos);
                let divisor = rhs[1]; // should be a unitless number (px slot)
                if divisor != 0.0 {
                    result = coeffs_mul(&result, 1.0 / divisor);
                }
            }
        } else {
            break;
        }
    }
    result
}

/// Atom level: parenthesized sub-expression or a single value with units.
fn calc_parse_atom(b: &[u8], pos: &mut usize) -> Coeffs {
    calc_skip_ws(b, pos);
    if *pos >= b.len() { return ZERO_COEFFS; }

    // Parenthesized sub-expression
    if b[*pos] == b'(' {
        *pos += 1; // skip '('
        let result = calc_parse_additive(b, pos);
        calc_skip_ws(b, pos);
        if *pos < b.len() && b[*pos] == b')' { *pos += 1; }
        return result;
    }

    // Parse a number (possibly negative) followed by optional unit
    let start = *pos;
    // Allow leading sign
    if *pos < b.len() && (b[*pos] == b'-' || b[*pos] == b'+') { *pos += 1; }
    // Allow leading dot like ".875rem"
    let mut has_digit = false;
    while *pos < b.len() && b[*pos].is_ascii_digit() { *pos += 1; has_digit = true; }
    if *pos < b.len() && b[*pos] == b'.' { *pos += 1; }
    while *pos < b.len() && b[*pos].is_ascii_digit() { *pos += 1; has_digit = true; }
    if !has_digit { return ZERO_COEFFS; }

    let num_end = *pos;
    let num_str = std::str::from_utf8(&b[start..num_end]).unwrap_or("0");
    let num: f32 = num_str.parse().unwrap_or(0.0);

    // Parse unit suffix
    let unit_start = *pos;
    while *pos < b.len() && b[*pos].is_ascii_alphabetic() { *pos += 1; }
    // Also allow '%'
    if *pos < b.len() && b[*pos] == b'%' { *pos += 1; }
    let unit = std::str::from_utf8(&b[unit_start..*pos]).unwrap_or("");

    let mut c = ZERO_COEFFS;
    match unit {
        "%"    => c[0] = num,
        "px"   => c[1] = num,
        "em"   => c[2] = num,
        "rem"  => c[3] = num,
        "vw"   => c[4] = num,
        "vh"   => c[5] = num,
        "vmin" => c[4] = num, // approximate
        "vmax" => c[4] = num, // approximate
        "pt"   => c[1] = num * 4.0 / 3.0,
        ""     => c[1] = num, // unitless → px
        _      => c[1] = num, // unknown unit → px
    }
    c
}

/// Find the index of the closing `)` that matches the opening `(` at position 0.
fn find_matching_paren(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => { depth -= 1; if depth == 0 { return Some(i); } }
            _ => {}
        }
    }
    None
}

/// Find the first comma outside nested parentheses.
fn find_top_level_comma(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => { if depth > 0 { depth -= 1; } }
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Split a string at top-level commas (not inside nested parentheses).
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => { if depth > 0 { depth -= 1; } }
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

pub fn parse_length_or_none(v: &str) -> CssLength {
    if v == "none" { CssLength::None } else { parse_length(v) }
}

pub fn parse_font_size(v: &str) -> CssLength {
    match v {
        "xx-small" => CssLength::Px(9.0),
        "x-small"  => CssLength::Px(10.0),
        "small"    => CssLength::Px(13.0),
        "medium"   => CssLength::Px(16.0),
        "large"    => CssLength::Px(18.0),
        "x-large"  => CssLength::Px(24.0),
        "xx-large" => CssLength::Px(32.0),
        "smaller"  => CssLength::Em(0.83),
        "larger"   => CssLength::Em(1.17),
        _          => parse_length(v),
    }
}

pub fn parse_line_height(v: &str) -> CssLength {
    if v == "normal" { return CssLength::Em(1.2); }
    // Unitless number: treat as em
    if let Ok(n) = v.parse::<f32>() { return CssLength::Em(n); }
    parse_length(v)
}

pub fn parse_overflow(v: &str) -> Overflow {
    match v {
        "hidden" => Overflow::Hidden,
        "scroll" => Overflow::Scroll,
        "auto"   => Overflow::Auto,
        _        => Overflow::Visible,
    }
}

pub fn parse_overscroll(v: &str) -> OverscrollBehavior {
    match v.trim() {
        "none"    => OverscrollBehavior::None,
        "contain" => OverscrollBehavior::Contain,
        _         => OverscrollBehavior::Auto,
    }
}

pub fn parse_border_style(v: &str) -> BorderStyle {
    try_parse_border_style(v).unwrap_or(BorderStyle::None)
}

fn try_parse_border_style(v: &str) -> Option<BorderStyle> {
    Some(match v {
        "none"   => BorderStyle::None,
        "hidden" => BorderStyle::Hidden,
        "solid"  => BorderStyle::Solid,
        "dashed" => BorderStyle::Dashed,
        "dotted" => BorderStyle::Dotted,
        "double" => BorderStyle::Double,
        "groove" => BorderStyle::Groove,
        "ridge"  => BorderStyle::Ridge,
        "inset"  => BorderStyle::Inset,
        "outset" => BorderStyle::Outset,
        _        => return None,
    })
}

/// Parse a CSS color value into a `Color`.
pub fn parse_color(v: &str) -> Option<Color> {
    let v = v.trim();

    // light-dark(light, dark) — use light value (we render in light mode)
    if v.starts_with("light-dark(") {
        if let Some(inner) = v.strip_prefix("light-dark(").and_then(|s| s.strip_suffix(')')) {
            let comma = inner.find(',')?;
            return parse_color(inner[..comma].trim());
        }
    }

    // Named colors
    let named = match v {
        "black"    => Some(Color::rgb(0, 0, 0)),
        "white"    => Some(Color::rgb(255, 255, 255)),
        "red"            => Some(Color::rgb(255,   0,   0)),
        "green"          => Some(Color::rgb(  0, 128,   0)),
        "blue"           => Some(Color::rgb(  0,   0, 255)),
        "yellow"         => Some(Color::rgb(255, 255,   0)),
        "orange"         => Some(Color::rgb(255, 165,   0)),
        "purple"         => Some(Color::rgb(128,   0, 128)),
        "pink"           => Some(Color::rgb(255, 192, 203)),
        "gray" | "grey"  => Some(Color::rgb(128, 128, 128)),
        "darkgray" | "darkgrey"   => Some(Color::rgb(169, 169, 169)),
        "lightgray"| "lightgrey"  => Some(Color::rgb(211, 211, 211)),
        "darkslategray" | "darkslategrey" => Some(Color::rgb( 47,  79,  79)),
        "slategray" | "slategrey" => Some(Color::rgb(112, 128, 144)),
        "lightslategray" | "lightslategrey" => Some(Color::rgb(119, 136, 153)),
        "dimgray" | "dimgrey"     => Some(Color::rgb(105, 105, 105)),
        "gainsboro"      => Some(Color::rgb(220, 220, 220)),
        "whitesmoke"     => Some(Color::rgb(245, 245, 245)),
        "silver"         => Some(Color::rgb(192, 192, 192)),
        "navy"           => Some(Color::rgb(  0,   0, 128)),
        "teal"           => Some(Color::rgb(  0, 128, 128)),
        "aqua" | "cyan"  => Some(Color::rgb(  0, 255, 255)),
        "fuchsia" | "magenta" => Some(Color::rgb(255, 0, 255)),
        "maroon"         => Some(Color::rgb(128,   0,   0)),
        "olive"          => Some(Color::rgb(128, 128,   0)),
        "lime"           => Some(Color::rgb(  0, 255,   0)),
        "darkred"        => Some(Color::rgb(139,   0,   0)),
        "darkgreen"      => Some(Color::rgb(  0, 100,   0)),
        "darkblue"       => Some(Color::rgb(  0,   0, 139)),
        "darkcyan"       => Some(Color::rgb(  0, 139, 139)),
        "darkmagenta"    => Some(Color::rgb(139,   0, 139)),
        "darkorange"     => Some(Color::rgb(255, 140,   0)),
        "darkviolet"     => Some(Color::rgb(148,   0, 211)),
        "darkgoldenrod"  => Some(Color::rgb(184, 134,  11)),
        "darkkhaki"      => Some(Color::rgb(189, 183, 107)),
        "darkturquoise"  => Some(Color::rgb(  0, 206, 209)),
        "darkorchid"     => Some(Color::rgb(153,  50, 204)),
        "darkseagreen"   => Some(Color::rgb(143, 188, 143)),
        "darksalmon"     => Some(Color::rgb(233, 150, 122)),
        "indigo"         => Some(Color::rgb( 75,   0, 130)),
        "violet"         => Some(Color::rgb(238, 130, 238)),
        "orchid"         => Some(Color::rgb(218, 112, 214)),
        "plum"           => Some(Color::rgb(221, 160, 221)),
        "thistle"        => Some(Color::rgb(216, 191, 216)),
        "lavender"       => Some(Color::rgb(230, 230, 250)),
        "mediumpurple"   => Some(Color::rgb(147, 112, 219)),
        "blueviolet"     => Some(Color::rgb(138,  43, 226)),
        "rebeccapurple"  => Some(Color::rgb(102,  51, 153)),
        "mediumblue"     => Some(Color::rgb(  0,   0, 205)),
        "royalblue"      => Some(Color::rgb( 65, 105, 225)),
        "cornflowerblue" => Some(Color::rgb(100, 149, 237)),
        "deepskyblue"    => Some(Color::rgb(  0, 191, 255)),
        "dodgerblue"     => Some(Color::rgb( 30, 144, 255)),
        "lightblue"      => Some(Color::rgb(173, 216, 230)),
        "lightskyblue"   => Some(Color::rgb(135, 206, 250)),
        "skyblue"        => Some(Color::rgb(135, 206, 235)),
        "steelblue"      => Some(Color::rgb( 70, 130, 180)),
        "cadetblue"      => Some(Color::rgb( 95, 158, 160)),
        "powderblue"     => Some(Color::rgb(176, 224, 230)),
        "lightcyan"      => Some(Color::rgb(224, 255, 255)),
        "paleturquoise"  => Some(Color::rgb(175, 238, 238)),
        "mediumturquoise"=> Some(Color::rgb( 72, 209, 204)),
        "turquoise"      => Some(Color::rgb( 64, 224, 208)),
        "aquamarine"     => Some(Color::rgb(127, 255, 212)),
        "mediumaquamarine" => Some(Color::rgb(102, 205, 170)),
        "lightgreen"     => Some(Color::rgb(144, 238, 144)),
        "limegreen"      => Some(Color::rgb( 50, 205,  50)),
        "mediumseagreen" => Some(Color::rgb( 60, 179, 113)),
        "seagreen"       => Some(Color::rgb( 46, 139,  87)),
        "forestgreen"    => Some(Color::rgb( 34, 139,  34)),
        "olivedrab"      => Some(Color::rgb(107, 142,  35)),
        "yellowgreen"    => Some(Color::rgb(154, 205,  50)),
        "chartreuse"     => Some(Color::rgb(127, 255,   0)),
        "lawngreen"      => Some(Color::rgb(124, 252,   0)),
        "greenyellow"    => Some(Color::rgb(173, 255,  47)),
        "palegreen"      => Some(Color::rgb(152, 251, 152)),
        "springgreen"    => Some(Color::rgb(  0, 255, 127)),
        "mediumspringgreen" => Some(Color::rgb(0, 250, 154)),
        "mintcream"      => Some(Color::rgb(245, 255, 250)),
        "honeydew"       => Some(Color::rgb(240, 255, 240)),
        "lemonchiffon"   => Some(Color::rgb(255, 250, 205)),
        "gold"           => Some(Color::rgb(255, 215,   0)),
        "goldenrod"      => Some(Color::rgb(218, 165,  32)),
        "palegoldenrod"  => Some(Color::rgb(238, 232, 170)),
        "wheat"          => Some(Color::rgb(245, 222, 179)),
        "moccasin"       => Some(Color::rgb(255, 228, 181)),
        "navajowhite"    => Some(Color::rgb(255, 222, 173)),
        "peachpuff"      => Some(Color::rgb(255, 218, 185)),
        "bisque"         => Some(Color::rgb(255, 228, 196)),
        "blanchedalmond" => Some(Color::rgb(255, 235, 205)),
        "papayawhip"     => Some(Color::rgb(255, 239, 213)),
        "antiquewhite"   => Some(Color::rgb(250, 235, 215)),
        "cornsilk"       => Some(Color::rgb(255, 248, 220)),
        "oldlace"        => Some(Color::rgb(253, 245, 230)),
        "floralwhite"    => Some(Color::rgb(255, 250, 240)),
        "ivory"          => Some(Color::rgb(255, 255, 240)),
        "beige"          => Some(Color::rgb(245, 245, 220)),
        "khaki"          => Some(Color::rgb(240, 230, 140)),
        "tan"            => Some(Color::rgb(210, 180, 140)),
        "burlywood"      => Some(Color::rgb(222, 184, 135)),
        "sandybrown"     => Some(Color::rgb(244, 164,  96)),
        "peru"           => Some(Color::rgb(205, 133,  63)),
        "chocolate"      => Some(Color::rgb(210, 105,  30)),
        "sienna"         => Some(Color::rgb(160,  82,  45)),
        "saddlebrown"    => Some(Color::rgb(139,  69,  19)),
        "brown"          => Some(Color::rgb(165,  42,  42)),
        "firebrick"      => Some(Color::rgb(178,  34,  34)),
        "crimson"        => Some(Color::rgb(220,  20,  60)),
        "tomato"         => Some(Color::rgb(255,  99,  71)),
        "coral"          => Some(Color::rgb(255, 127,  80)),
        "salmon"         => Some(Color::rgb(250, 128, 114)),
        "lightsalmon"    => Some(Color::rgb(255, 160, 122)),
        "lightcoral"     => Some(Color::rgb(240, 128, 128)),
        "lightyellow"    => Some(Color::rgb(255, 255, 224)),
        "lightgoldenrodyellow" => Some(Color::rgb(250, 250, 210)),
        "lightseagreen"  => Some(Color::rgb( 32, 178, 170)),
        "lightsteelblue" => Some(Color::rgb(176, 196, 222)),
        "orangered"      => Some(Color::rgb(255,  69,   0)),
        "hotpink"        => Some(Color::rgb(255, 105, 180)),
        "deeppink"       => Some(Color::rgb(255,  20, 147)),
        "lightpink"      => Some(Color::rgb(255, 182, 193)),
        "palevioletred"  => Some(Color::rgb(219, 112, 147)),
        "mediumvioletred"=> Some(Color::rgb(199,  21, 133)),
        "rosybrown"      => Some(Color::rgb(188, 143, 143)),
        "mistyrose"      => Some(Color::rgb(255, 228, 225)),
        "lavenderblush"  => Some(Color::rgb(255, 240, 245)),
        "aliceblue"      => Some(Color::rgb(240, 248, 255)),
        "ghostwhite"     => Some(Color::rgb(248, 248, 255)),
        "azure"          => Some(Color::rgb(240, 255, 255)),
        "snow"           => Some(Color::rgb(255, 250, 250)),
        "seashell"       => Some(Color::rgb(255, 245, 238)),
        "linen"          => Some(Color::rgb(250, 240, 230)),
        "transparent"    => Some(Color::TRANSPARENT),
        "currentcolor"   => None,  // can't resolve without context
        _ => None,
    };
    if named.is_some() { return named; }

    // #rrggbb or #rgb
    if v.starts_with('#') {
        let hex = &v[1..];
        return match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Color::rgb(r, g, b))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Color::rgba(r, g, b, a))
            }
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                Some(Color::rgb(r, g, b))
            }
            4 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                let a = u8::from_str_radix(&hex[3..4], 16).ok()? * 17;
                Some(Color::rgba(r, g, b, a))
            }
            _ => None,
        };
    }

    // rgb(r, g, b) or rgba(r, g, b, a)
    if v.starts_with("rgb") {
        let inner = v.trim_start_matches("rgba").trim_start_matches("rgb")
            .trim_start_matches('(').trim_end_matches(')');
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() >= 3 {
            let parse_channel = |s: &str| -> u8 {
                let s = s.trim();
                if s.ends_with('%') {
                    (s[..s.len()-1].parse::<f32>().unwrap_or(0.0) / 100.0 * 255.0) as u8
                } else {
                    s.parse::<f32>().unwrap_or(0.0).round() as u8
                }
            };
            let r = parse_channel(parts[0]);
            let g = parse_channel(parts[1]);
            let b = parse_channel(parts[2]);
            let a = if parts.len() >= 4 {
                (parts[3].trim().parse::<f32>().unwrap_or(1.0) * 255.0) as u8
            } else {
                255
            };
            return Some(Color::rgba(r, g, b, a));
        }
    }

    // hsl (simplified: convert hsl to rgb)
    if v.starts_with("hsl") {
        let inner = v.trim_start_matches("hsla").trim_start_matches("hsl")
            .trim_start_matches('(').trim_end_matches(')');
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() >= 3 {
            let h = parts[0].trim().parse::<f32>().unwrap_or(0.0) / 360.0;
            let s = parts[1].trim().trim_end_matches('%').parse::<f32>().unwrap_or(0.0) / 100.0;
            let l = parts[2].trim().trim_end_matches('%').parse::<f32>().unwrap_or(0.0) / 100.0;
            let a = if parts.len() >= 4 {
                (parts[3].trim().parse::<f32>().unwrap_or(1.0) * 255.0) as u8
            } else {
                255
            };
            let (r, g, b) = hsl_to_rgb(h, s, l);
            return Some(Color::rgba((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, a));
        }
    }

    None
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s == 0.0 { return (l, l, l); }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);
    (r, g, b)
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
    if t < 1.0 / 2.0 { return q; }
    if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
    p
}

// ─── Grid Track Parsers ───────────────────────────────────────────────────────

/// Parse a single grid track size token.
pub fn parse_single_track(v: &str) -> GridTrackSize {
    let v = v.trim();
    if v == "auto" { return GridTrackSize::auto(); }
    if v == "min-content" { return GridTrackSize { kind: GridTrackKind::MinContent, ..Default::default() }; }
    if v == "max-content" { return GridTrackSize { kind: GridTrackKind::MaxContent, ..Default::default() }; }
    if v.ends_with("fr") {
        let fr: f32 = v[..v.len()-2].parse().unwrap_or(1.0);
        return GridTrackSize::fr(fr);
    }
    if v.ends_with('%') {
        let pct: f32 = v[..v.len()-1].parse().unwrap_or(0.0);
        return GridTrackSize::percent(pct);
    }
    if v.ends_with("px") {
        let px: f32 = v[..v.len()-2].parse().unwrap_or(0.0);
        return GridTrackSize::fixed(px);
    }
    if v.starts_with("calc(") {
        let len = parse_length(v);
        return GridTrackSize {
            kind: GridTrackKind::Calc,
            calc_length: Some(len),
            ..Default::default()
        };
    }
    if v.starts_with("minmax(") {
        let inner = &v[7..v.len()-1];
        let parts: Vec<&str> = inner.splitn(2, ',').collect();
        if parts.len() == 2 {
            let min_t = parse_single_track(parts[0].trim());
            let max_t = parse_single_track(parts[1].trim());
            return GridTrackSize {
                kind: GridTrackKind::MinMax,
                value: 0.0,
                min_kind: min_t.kind,
                min_value: min_t.value,
                max_kind: max_t.kind,
                max_value: max_t.value,
                calc_length: None,
            };
        }
    }
    if v.starts_with("fit-content(") {
        let inner = &v[12..v.len()-1];
        let t = parse_single_track(inner.trim());
        return GridTrackSize {
            kind: GridTrackKind::FitContent,
            value: t.value,
            max_kind: t.kind,
            max_value: t.value,
            ..Default::default()
        };
    }
    // unitless number → px
    if let Ok(n) = v.parse::<f32>() {
        return GridTrackSize::fixed(n);
    }
    GridTrackSize::auto()
}

/// Parse a grid-template-columns/rows value into Vec<GridTrackSize>.
/// Also extracts named grid lines into line_names: name → Vec<line_index> (0-based).
/// Handles repeat(), minmax(), fr, px, %, auto, min-content, max-content.
/// auto_repeat_cols receives any auto-fill/auto-fit tracks.
pub fn parse_track_list(
    v: &str,
    auto_repeat_cols: &mut Vec<GridTrackSize>,
) -> Vec<GridTrackSize> {
    let mut line_names = std::collections::HashMap::new();
    parse_track_list_with_names(v, auto_repeat_cols, &mut line_names)
}

/// Like parse_track_list but also populates a name→line-number map.
pub fn parse_track_list_with_names(
    v: &str,
    auto_repeat_cols: &mut Vec<GridTrackSize>,
    line_names: &mut std::collections::HashMap<String, Vec<usize>>,
) -> Vec<GridTrackSize> {
    if v.is_empty() { return Vec::new(); }
    if v.trim() == "subgrid" { return vec![GridTrackSize::subgrid()]; }
    let tokens = tokenize_track_list(v);
    let mut result = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i].trim();
        if t.starts_with('[') && t.ends_with(']') {
            // Named line: [name1 name2 ...]
            let inner = &t[1..t.len()-1];
            let line_idx = result.len(); // line is BEFORE the next track
            for name in inner.split_whitespace() {
                line_names.entry(name.to_string())
                    .or_insert_with(Vec::new)
                    .push(line_idx);
            }
        } else if t.starts_with("repeat(") || (i + 1 < tokens.len() && t == "repeat") {
            let repeat_str = if t.starts_with("repeat(") && t.ends_with(')') {
                t.to_string()
            } else {
                t.to_string()
            };
            // Strip "repeat(" prefix and single trailing ")" — not trim which strips multiple
            let stripped = repeat_str.strip_prefix("repeat(").unwrap_or(&repeat_str);
            let inner = stripped.strip_suffix(')').unwrap_or(stripped);
            // Find top-level comma (not inside parens)
            let comma = {
                let mut depth = 0;
                let mut pos = None;
                for (i, ch) in inner.chars().enumerate() {
                    if ch == '(' { depth += 1; }
                    if ch == ')' { depth -= 1; }
                    if ch == ',' && depth == 0 { pos = Some(i); break; }
                }
                pos.unwrap_or(0)
            };
            let count_str = inner[..comma].trim();
            let track_str = inner[comma+1..].trim();
            let track = parse_single_track(track_str);
            if count_str == "auto-fill" || count_str == "auto-fit" {
                auto_repeat_cols.push(track.clone());
            } else {
                let count = if let Ok(n) = count_str.parse::<usize>() {
                    n
                } else {
                    // Handle calc() in repeat count, e.g. repeat(calc(5 - 1), ...)
                    let resolved = parse_length(count_str).resolve(16.0, 0.0, 16.0);
                    if resolved > 0.0 { resolved as usize } else { 1 }
                };
                for _ in 0..count {
                    result.push(track.clone());
                }
            }
        } else if !t.is_empty() {
            result.push(parse_single_track(t));
        }
        i += 1;
    }
    result
}

/// Tokenize a track list, keeping repeat(...) and [...] as single tokens.
fn tokenize_track_list(v: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    for ch in v.chars() {
        match ch {
            '[' => {
                // If there's content before '[', push it as a separate token
                if bracket_depth == 0 && paren_depth == 0 {
                    let s = current.trim().to_string();
                    if !s.is_empty() { tokens.push(s); }
                    current = String::new();
                }
                bracket_depth += 1; current.push(ch);
            }
            ']' => {
                if bracket_depth > 0 { bracket_depth -= 1; }
                current.push(ch);
                if bracket_depth == 0 && paren_depth == 0 {
                    tokens.push(current.trim().to_string());
                    current = String::new();
                }
            }
            '(' => { paren_depth += 1; current.push(ch); }
            ')' => {
                if paren_depth > 0 { paren_depth -= 1; }
                current.push(ch);
                if paren_depth == 0 && bracket_depth == 0 {
                    tokens.push(current.trim().to_string());
                    current = String::new();
                }
            }
            ' ' | '\t' | '\n' if paren_depth == 0 && bracket_depth == 0 => {
                let s = current.trim().to_string();
                if !s.is_empty() { tokens.push(s); }
                current = String::new();
            }
            _ => { current.push(ch); }
        }
    }
    let s = current.trim().to_string();
    if !s.is_empty() { tokens.push(s); }
    tokens
}

/// Parse grid-template-areas string.
/// Input: `"a a b" "a a b" "c c b"` → Vec<Vec<String>>
pub fn parse_grid_template_areas(v: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    // Each quoted string is a row
    let mut rest = v.trim();
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.starts_with('"') || rest.starts_with('\'') {
            let q = rest.chars().next().unwrap();
            let end = rest[1..].find(q).unwrap_or(rest.len() - 1);
            let row_str = &rest[1..end+1];
            let cells: Vec<String> = row_str.split_whitespace().map(|s| s.to_string()).collect();
            if !cells.is_empty() { rows.push(cells); }
            rest = &rest[end + 2..];
        } else {
            break;
        }
    }
    rows
}

/// Parse a CSS grid line value.
/// Returns: (numeric_value, named_reference)
/// numeric: positive = explicit 1-based line, 0 = auto,
/// negative > -10000 = negative line number, <= -10000 = span (encoded).
/// named: non-empty if referencing a named line like "content-start" or area "content".
pub fn parse_grid_line_named(v: &str) -> (i32, String) {
    let v = v.trim();
    if v == "auto" || v.is_empty() { return (0, String::new()); }
    if v.starts_with("span ") {
        let rest = v[5..].trim();
        let n: i32 = rest.parse().unwrap_or(1);
        return (-(n + 10000), String::new());
    }
    if let Ok(n) = v.parse::<i32>() {
        return (n, String::new());
    }
    // Named line reference (e.g. "content", "content-start", "title-end")
    (0, v.to_string())
}

/// Convenience wrapper for parse_grid_line_named that discards the name.
pub fn parse_grid_line(v: &str) -> i32 {
    parse_grid_line_named(v).0
}

// ─── Media Query Evaluator ───────────────────────────────────────────────────

/// Evaluate a CSS @media condition string.
/// Returns true if the condition matches the given viewport dimensions.
/// `condition` is the full text after "@media" (trimmed).
pub fn evaluate_media(condition: &str, vw: f32, vh: f32) -> bool {
    let cond = condition.trim();
    if cond.is_empty() { return true; }

    // Handle comma-separated list at top level (OR semantics)
    // We first split on `and`/`or` outside parens, then check named types.
    // But comma is always OR at the top level.
    {
        let mut depth = 0usize;
        let bytes = cond.as_bytes();
        let mut comma_pos: Option<usize> = None;
        for (i, &b) in bytes.iter().enumerate() {
            match b {
                b'(' => depth += 1,
                b')' => { if depth > 0 { depth -= 1; } }
                b',' if depth == 0 => { comma_pos = Some(i); break; }
                _ => {}
            }
        }
        if let Some(pos) = comma_pos {
            let left  = &cond[..pos];
            let right = &cond[pos+1..];
            return evaluate_media(left.trim(), vw, vh) || evaluate_media(right.trim(), vw, vh);
        }
    }

    // Handle `not` prefix (before `and`/`or` splitting)
    if let Some(rest) = cond.strip_prefix("not ") {
        return !evaluate_media(rest.trim(), vw, vh);
    }

    // Handle `and` combinator outside parens
    if let Some(idx) = find_keyword_outside_parens(cond, " and ") {
        let left  = &cond[..idx];
        let right = &cond[idx + 5..];
        return evaluate_media(left.trim(), vw, vh) && evaluate_media(right.trim(), vw, vh);
    }

    // Handle `or` combinator outside parens
    if let Some(idx) = find_keyword_outside_parens(cond, " or ") {
        let left  = &cond[..idx];
        let right = &cond[idx + 4..];
        return evaluate_media(left.trim(), vw, vh) || evaluate_media(right.trim(), vw, vh);
    }

    // Named media types (no parens)
    if !cond.starts_with('(') {
        return match cond.to_ascii_lowercase().as_str() {
            "screen" | "all" => true,
            "print"  => false,
            _ => true,  // unknown media type — fail-open
        };
    }

    // Strip outer parens for feature queries
    let inner = if cond.starts_with('(') && cond.ends_with(')') {
        &cond[1..cond.len()-1]
    } else {
        cond
    };
    let lower = inner.to_ascii_lowercase();
    let lower = lower.trim();

    if let Some(rest) = lower.strip_prefix("min-width:") {
        return vw >= parse_media_px(rest.trim());
    }
    if let Some(rest) = lower.strip_prefix("max-width:") {
        return vw <= parse_media_px(rest.trim());
    }
    if let Some(rest) = lower.strip_prefix("min-height:") {
        return vh >= parse_media_px(rest.trim());
    }
    if let Some(rest) = lower.strip_prefix("max-height:") {
        return vh <= parse_media_px(rest.trim());
    }
    if let Some(rest) = lower.strip_prefix("orientation:") {
        return match rest.trim() {
            "landscape" => vw > vh,
            "portrait"  => vh >= vw,
            _ => true,
        };
    }
    if let Some(rest) = lower.strip_prefix("prefers-color-scheme:") {
        return match rest.trim() {
            "light" => true,
            "dark"  => false,
            _ => true,
        };
    }
    if let Some(rest) = lower.strip_prefix("hover:") {
        return match rest.trim() { "hover" => true, "none" => false, _ => true };
    }
    if let Some(rest) = lower.strip_prefix("pointer:") {
        return match rest.trim() { "fine" => true, "coarse" | "none" => false, _ => true };
    }
    if let Some(rest) = lower.strip_prefix("min-resolution:") {
        let s = rest.trim().trim_end_matches("dpi").trim_end_matches("dpcm").trim();
        let dpi: f32 = s.parse().unwrap_or(0.0);
        return dpi <= 96.0;
    }
    if let Some(rest) = lower.strip_prefix("max-resolution:") {
        let s = rest.trim().trim_end_matches("dpi").trim_end_matches("dpcm").trim();
        let dpi: f32 = s.parse().unwrap_or(0.0);
        return dpi >= 96.0;
    }
    // Modern range syntax: `width >= 300px`, `width > 300px`, etc.
    fn parse_media_range(expr: &str, dim: f32) -> Option<bool> {
        let e = expr.trim();
        if let Some(rest) = e.strip_prefix(">=") { return Some(dim >= parse_media_px(rest.trim())); }
        if let Some(rest) = e.strip_prefix("<=") { return Some(dim <= parse_media_px(rest.trim())); }
        if let Some(rest) = e.strip_prefix('>')  { return Some(dim >  parse_media_px(rest.trim())); }
        if let Some(rest) = e.strip_prefix('<')  { return Some(dim <  parse_media_px(rest.trim())); }
        if let Some(rest) = e.strip_prefix(':')  { return Some((dim - parse_media_px(rest.trim())).abs() < 0.5); }
        None
    }
    if let Some(rest) = lower.strip_prefix("width")  { if let Some(v) = parse_media_range(rest, vw) { return v; } }
    if let Some(rest) = lower.strip_prefix("height") { if let Some(v) = parse_media_range(rest, vh) { return v; } }
    if let Some(rest) = lower.strip_prefix("inline-size")  { if let Some(v) = parse_media_range(rest, vw) { return v; } }
    if let Some(rest) = lower.strip_prefix("block-size")   { if let Some(v) = parse_media_range(rest, vh) { return v; } }

    // Unknown feature — fail-open
    true
}

/// Find byte index of `keyword` in `s` where it is not inside parentheses.
fn find_keyword_outside_parens(s: &str, keyword: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let kw = keyword.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i + kw.len() <= bytes.len() {
        match bytes[i] {
            b'(' => { depth += 1; i += 1; }
            b')' => { if depth > 0 { depth -= 1; } i += 1; }
            _ => {
                if depth == 0 && bytes[i..].starts_with(kw) {
                    return Some(i);
                }
                i += 1;
            }
        }
    }
    None
}

fn parse_media_px(s: &str) -> f32 {
    let s = s.trim();
    if s.ends_with("px") {
        s[..s.len()-2].trim().parse().unwrap_or(0.0)
    } else if s.ends_with("em") {
        s[..s.len()-2].trim().parse::<f32>().unwrap_or(0.0) * 16.0
    } else {
        s.parse().unwrap_or(0.0)
    }
}

// ─── Container Query Evaluation ──────────────────────────────────────────────

/// Evaluate a `@container` condition string against known container dimensions.
///
/// Supports:
/// - Legacy syntax: `(min-width: Xpx)`, `(max-width: Xpx)`, `(min-height: Xpx)`, `(max-height: Xpx)`
/// - Modern range syntax: `(width > Xpx)`, `(width >= Xpx)`, `(width < Xpx)`, `(width <= Xpx)`
/// - Logical: `and`, `or`, `not`
pub fn evaluate_container(condition: &str, w: f32, h: f32) -> bool {
    let cond = condition.trim();
    if cond.is_empty() { return true; }

    // Comma = OR at top level
    {
        let mut depth = 0usize;
        let bytes = cond.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            match b {
                b'(' => depth += 1,
                b')' => { if depth > 0 { depth -= 1; } }
                b',' if depth == 0 => {
                    return evaluate_container(&cond[..i], w, h)
                        || evaluate_container(&cond[i+1..], w, h);
                }
                _ => {}
            }
        }
    }

    if let Some(rest) = cond.strip_prefix("not ") {
        return !evaluate_container(rest.trim(), w, h);
    }
    if let Some(idx) = find_keyword_outside_parens(cond, " and ") {
        return evaluate_container(&cond[..idx], w, h) && evaluate_container(&cond[idx+5..], w, h);
    }
    if let Some(idx) = find_keyword_outside_parens(cond, " or ") {
        return evaluate_container(&cond[..idx], w, h) || evaluate_container(&cond[idx+4..], w, h);
    }

    // Strip outer parens
    let inner = if cond.starts_with('(') && cond.ends_with(')') {
        &cond[1..cond.len()-1]
    } else {
        cond
    };
    let lower = inner.to_ascii_lowercase();
    let lower = lower.trim();

    // Legacy min-/max- syntax
    if let Some(rest) = lower.strip_prefix("min-width:")  { return w >= parse_media_px(rest.trim()); }
    if let Some(rest) = lower.strip_prefix("max-width:")  { return w <= parse_media_px(rest.trim()); }
    if let Some(rest) = lower.strip_prefix("min-height:") { return h >= parse_media_px(rest.trim()); }
    if let Some(rest) = lower.strip_prefix("max-height:") { return h <= parse_media_px(rest.trim()); }

    // Modern range syntax: `width >= 300px`, `width > 300px`, etc.
    fn parse_range(expr: &str, dim: f32) -> Option<bool> {
        let e = expr.trim();
        if let Some(rest) = e.strip_prefix(">=") { return Some(dim >= parse_media_px(rest.trim())); }
        if let Some(rest) = e.strip_prefix("<=") { return Some(dim <= parse_media_px(rest.trim())); }
        if let Some(rest) = e.strip_prefix('>')  { return Some(dim >  parse_media_px(rest.trim())); }
        if let Some(rest) = e.strip_prefix('<')  { return Some(dim <  parse_media_px(rest.trim())); }
        if let Some(rest) = e.strip_prefix(':')  { return Some((dim - parse_media_px(rest.trim())).abs() < 0.5); }
        None
    }
    if let Some(rest) = lower.strip_prefix("width")  { if let Some(v) = parse_range(rest, w) { return v; } }
    if let Some(rest) = lower.strip_prefix("height") { if let Some(v) = parse_range(rest, h) { return v; } }
    if let Some(rest) = lower.strip_prefix("inline-size")  { if let Some(v) = parse_range(rest, w) { return v; } }
    if let Some(rest) = lower.strip_prefix("block-size")   { if let Some(v) = parse_range(rest, h) { return v; } }

    // Unknown — fail-open
    true
}

// ─── Container Cascade Pass ───────────────────────────────────────────────────

/// An entry on the container ancestor stack built during `apply_container_cascade_tree`.
#[derive(Clone)]
pub struct ContainerEntry {
    pub width:  f32,
    pub height: f32,
    pub name:   String,
}

/// Walk `node` and all its descendants applying any `@container` rules whose
/// condition matches the nearest container ancestor in `container_stack`.
///
/// This is called as a post-layout pass (after box sizes are known) so that
/// container dimensions are available for condition evaluation.
///
/// Returns `true` if any styles were changed (used to decide whether a
/// second layout pass is needed).
pub fn apply_container_cascade_tree(
    node: &mut crate::types::HtmlBox,
    stylesheet: &Stylesheet,
    container_stack: &[ContainerEntry],
    ancestors: &[AncestorInfo],
    child_index: usize,
    sibling_count: usize,
    type_child_index: usize,
    type_sibling_count: usize,
    root_font_px: f32,
    vw: f32,
    vh: f32,
    focused_box: u32,
    keyboard_focus: bool,
) -> bool {
    // Create owned Vecs once at the top level; the recursive inner function
    // reuses them via push/pop so no per-node heap allocation is needed.
    let mut cs  = container_stack.to_vec();
    let mut anc = ancestors.to_vec();
    apply_container_cascade_inner(
        node, stylesheet, &mut cs, &mut anc,
        child_index, sibling_count, type_child_index, type_sibling_count,
        root_font_px, vw, vh, focused_box, keyboard_focus,
    )
}

fn apply_container_cascade_inner(
    node: &mut crate::types::HtmlBox,
    stylesheet: &Stylesheet,
    container_stack: &mut Vec<ContainerEntry>,
    ancestors: &mut Vec<AncestorInfo>,
    child_index: usize,
    sibling_count: usize,
    type_child_index: usize,
    type_sibling_count: usize,
    root_font_px: f32,
    vw: f32,
    vh: f32,
    focused_box: u32,
    keyboard_focus: bool,
) -> bool {
    use crate::types::ContainerType;

    let mut changed = false;

    // Apply matching container rules to this element
    if !container_stack.is_empty() {
        let empty_hover = std::collections::HashSet::new();
        let match_ctx = MatchContext {
            focused_box,
            keyboard_focus,
            type_child_index,
            type_sibling_count,
            html_box: Some(node),
            hover_chain: &empty_hover,
            element_id: node.node_id,
        };
        let mut cont_matched: Vec<(u32, HashMap<String, String>)> = Vec::new();
        for rule in &stylesheet.rules {
            if rule.container_condition.is_empty() { continue; }
            if !rule.media_condition.is_empty() && !evaluate_media(&rule.media_condition, vw, vh) { continue; }
            // Find nearest container that matches the rule's name
            let ctx = if rule.container_name.is_empty() {
                container_stack.last()
            } else {
                container_stack.iter().rev().find(|c| c.name == rule.container_name)
            };
            let ctx = match ctx { Some(c) => c, None => continue };
            if !evaluate_container(&rule.container_condition, ctx.width, ctx.height) { continue; }
            // Full selector matching (same logic as apply_cascade_inner)
            let has_hover   = rule.selectors.iter().any(|s| s.parts.iter().any(|p| matches!(p, SelectorPart::PseudoClass(n) if n == "hover")));
            let has_active  = rule.selectors.iter().any(|s| s.parts.iter().any(|p| matches!(p, SelectorPart::PseudoClass(n) if n == "active")));
            if has_hover || has_active { continue; }  // state pseudo-class rules are handled separately
            for sel in &rule.selectors {
                if sel.matches_with_ancestors_ctx(node, child_index, sibling_count, ancestors, &match_ctx) {
                    if rule.pseudo_element == PseudoElement::None {
                        let mut merged = rule.declarations.clone();
                        for (k, v) in &rule.important_declarations {
                            merged.insert(k.clone(), v.clone());
                        }
                        cont_matched.push((rule.specificity, merged));
                    }
                    break;
                }
            }
        }
        if !cont_matched.is_empty() {
            changed = true;
            cont_matched.sort_by_key(|(sp, _)| *sp);
            for (_, decls) in &cont_matched {
                for (prop, val) in decls {
                    let resolved = resolve_var_references(val, &stylesheet.variables);
                    apply_property(&mut node.style, prop, &resolved);
                }
            }
            // Mark layout dirty so the subtree pruning doesn't suppress the
            // geometry changes caused by these newly applied container rules.
            node.layout.layout_dirty = true;
        }
    }

    // Update container stack: if this element is a container, push it
    // Push this element as a container ancestor (if it qualifies), recurse, pop.
    let pushed_container = !matches!(node.style.container_type, ContainerType::Normal);
    if pushed_container {
        container_stack.push(ContainerEntry {
            width:  node.layout.content_rect.w,
            height: node.layout.content_rect.h,
            name:   node.style.container_name.clone(),
        });
    }

    let n_children = node.children.len();
    if n_children == 0 {
        if pushed_container { container_stack.pop(); }
        return changed;
    }

    // Push this element as an ancestor for children (mirrors apply_cascade_inner).
    ancestors.push(AncestorInfo {
        tag:              node.tag.clone(),
        attributes:       node.attributes.clone(),
        child_index,
        sibling_count,
        type_child_index,
        type_sibling_count,
        node_id:          node.node_id,
    });

    // O(n) type counting (was O(n²) with per-child filter passes).
    let child_tags: Vec<String> = node.children.iter().map(|c| c.tag.to_ascii_lowercase()).collect();
    let mut type_running: HashMap<&str, usize> = HashMap::new();
    let type_counts: Vec<usize> = child_tags.iter().map(|tag| {
        let slot = type_running.entry(tag.as_str()).or_insert(0);
        let idx  = *slot;
        *slot += 1;
        idx
    }).collect();
    let type_totals: Vec<usize> = child_tags.iter().map(|tag| {
        *type_running.get(tag.as_str()).unwrap_or(&0)
    }).collect();

    for (i, child) in node.children.iter_mut().enumerate() {
        let c = apply_container_cascade_inner(
            child, stylesheet, container_stack, ancestors,
            i, n_children, type_counts[i], type_totals[i],
            root_font_px, vw, vh, focused_box, keyboard_focus,
        );
        if c { changed = true; }
    }

    // If any descendant changed, mark this node dirty too.
    // This prevents the layout subtree pruning from skipping an ancestor whose
    // content width is unchanged while a child still needs re-layout.
    if changed { node.layout.layout_dirty = true; }

    ancestors.pop();
    if pushed_container { container_stack.pop(); }
    changed
}

// ─── CSS Cascade ─────────────────────────────────────────────────────────────

/// Apply a stylesheet to all boxes in the tree (cascade + inheritance).
pub fn apply_cascade(root: &mut crate::types::HtmlBox, stylesheet: &Stylesheet,
                     parent_style: Option<&ComputedStyle>, root_font_px: f32) {
    apply_cascade_vp(root, stylesheet, parent_style, root_font_px, 0.0, 0.0, 0, false);
}

/// Apply a stylesheet with viewport size and focused element for media queries and :focus selectors.
///
/// `keyboard_focus` controls whether `:focus-visible` matches: pass `true` only when
/// focus was moved by keyboard (Tab/Shift+Tab), `false` for mouse-click focus.
///
/// **Note**: call `stylesheet.rebuild_index()` before this if rules were added since last cascade.
pub fn apply_cascade_vp(
    root: &mut crate::types::HtmlBox,
    stylesheet: &Stylesheet,
    parent_style: Option<&ComputedStyle>,
    root_font_px: f32,
    vw: f32,
    vh: f32,
    focused_box: u32,
    keyboard_focus: bool,
) {
    let empty_hover = std::collections::HashSet::new();
    apply_cascade_vp_hover(root, stylesheet, parent_style, root_font_px, vw, vh, focused_box, keyboard_focus, &empty_hover);
}

/// Cascade with hover chain: elements in hover_chain will match :hover pseudo-class.
///
/// When the stylesheet has more than 1000 rules, automatically uses a parallel
/// selector-matching pass (via Rayon) to speed up large pages.
pub fn apply_cascade_vp_hover(
    root: &mut crate::types::HtmlBox,
    stylesheet: &Stylesheet,
    parent_style: Option<&ComputedStyle>,
    root_font_px: f32,
    vw: f32,
    vh: f32,
    focused_box: u32,
    keyboard_focus: bool,
    hover_chain: &std::collections::HashSet<u32>,
) {
    // Use parallel cascade when the stylesheet is large enough to justify the overhead.
    if stylesheet.rules.len() > 1000 {
        apply_cascade_parallel(root, stylesheet, parent_style, root_font_px, vw, vh, focused_box, keyboard_focus, hover_chain);
        return;
    }
    // A single Vec is reused for the entire tree traversal (push/pop per node)
    // instead of cloning the ancestor list at every level — O(depth) allocations
    // instead of O(nodes × depth).
    let mut ancestors: Vec<AncestorInfo> = Vec::new();
    let mut candidates_buf: Vec<usize> = Vec::new();
    let mut counters: HashMap<String, Vec<i32>> = HashMap::new();
    apply_cascade_inner(root, stylesheet, parent_style, root_font_px, &mut ancestors, 0, 1, 0, 1, vw, vh, focused_box, keyboard_focus, &stylesheet.variables, &mut candidates_buf, &mut counters, hover_chain);
}

// ─── Incremental Hover Cascade ──────────────────────────────────────────────

/// Mark nodes affected by a hover change by walking the tree.
/// Sets `cascade_dirty` on nodes whose hover state toggled (symmetric difference),
/// and `has_dirty_descendant` on their ancestors (the hover chain path).
pub fn mark_hover_dirty(
    root: &mut crate::types::HtmlBox,
    old_chain: &std::collections::HashSet<u32>,
    new_chain: &std::collections::HashSet<u32>,
    has_hover_descendant_rules: bool,
    hover_sensitive: &std::collections::HashSet<u32>,
) {
    // Nodes whose hover state actually changed (in one chain but not both)
    let toggled: std::collections::HashSet<u32> = old_chain.symmetric_difference(new_chain).copied().collect();
    // All nodes on the path (for has_dirty_descendant traversal)
    let path: std::collections::HashSet<u32> = old_chain.union(new_chain).copied().collect();

    fn walk(node: &mut crate::types::HtmlBox, toggled: &std::collections::HashSet<u32>,
            path: &std::collections::HashSet<u32>, has_hover_desc: bool,
            sensitive: &std::collections::HashSet<u32>) -> bool {
        let mut any_dirty = false;
        // Only mark cascade_dirty if this node is hover-sensitive (has hover CSS rules)
        if toggled.contains(&node.node_id) && (sensitive.is_empty() || sensitive.contains(&node.node_id)) {
            node.cascade_dirty = true;
            any_dirty = true;
            if has_hover_desc {
                mark_children_cascade_dirty(node);
            }
        }
        if path.contains(&node.node_id) {
            node.has_dirty_descendant = true;
            any_dirty = true;
        }
        for child in &mut node.children {
            if walk(child, toggled, path, has_hover_desc, sensitive) {
                node.has_dirty_descendant = true;
                any_dirty = true;
            }
        }
        any_dirty
    }

    walk(root, &toggled, &path, has_hover_descendant_rules, hover_sensitive);
}

fn mark_children_cascade_dirty(node: &mut crate::types::HtmlBox) {
    for child in &mut node.children {
        child.cascade_dirty = true;
        mark_children_cascade_dirty(child);
    }
}

/// Clear cascade_dirty and has_dirty_descendant flags after incremental cascade.
/// Clear cascade_dirty flags after cascade. Preserves has_dirty_descendant
/// for the layout pass (propagate_dirty uses it to skip clean subtrees).
pub fn clear_cascade_dirty(node: &mut crate::types::HtmlBox) {
    if !node.cascade_dirty && !node.has_dirty_descendant { return; }
    node.cascade_dirty = false;
    // Note: has_dirty_descendant is intentionally NOT cleared here — layout needs it.
    // It gets cleared after layout in clear_layout_dirty().
    for child in &mut node.children {
        clear_cascade_dirty(child);
    }
}

/// Clear has_dirty_descendant flags after layout completes.
pub fn clear_descendant_dirty(node: &mut crate::types::HtmlBox) {
    if !node.has_dirty_descendant { return; }
    node.has_dirty_descendant = false;
    for child in &mut node.children {
        clear_descendant_dirty(child);
    }
}

/// Incremental hover cascade: single tree walk that skips clean subtrees.
/// Only re-cascades nodes with `cascade_dirty` flag set. Nodes with only
/// `has_dirty_descendant` are traversed but not re-cascaded.
/// Call `mark_hover_dirty()` before and `clear_cascade_dirty()` after.
pub fn apply_cascade_incremental(
    root: &mut crate::types::HtmlBox,
    stylesheet: &Stylesheet,
    parent_style: Option<&ComputedStyle>,
    root_font_px: f32,
    vw: f32,
    vh: f32,
    focused_box: u32,
    keyboard_focus: bool,
    hover_chain: &std::collections::HashSet<u32>,
) {
    let mut ancestors: Vec<AncestorInfo> = Vec::new();
    let mut candidates_buf: Vec<usize> = Vec::new();
    let mut counters: HashMap<String, Vec<i32>> = HashMap::new();
    apply_cascade_incremental_walk(
        root, stylesheet, parent_style, root_font_px,
        &mut ancestors, 0, 1, 0, 1,
        vw, vh, focused_box, keyboard_focus,
        &stylesheet.variables, &mut candidates_buf, &mut counters,
        hover_chain,
    );
}

fn apply_cascade_incremental_walk(
    node: &mut crate::types::HtmlBox,
    stylesheet: &Stylesheet,
    parent_style: Option<&ComputedStyle>,
    root_font_px: f32,
    ancestors: &mut Vec<AncestorInfo>,
    child_index: usize,
    sibling_count: usize,
    type_child_index: usize,
    type_sibling_count: usize,
    vw: f32,
    vh: f32,
    focused_box: u32,
    keyboard_focus: bool,
    inherited_vars: &HashMap<String, String>,
    candidates_buf: &mut Vec<usize>,
    counters: &mut HashMap<String, Vec<i32>>,
    hover_chain: &std::collections::HashSet<u32>,
) {
    // SKIP: neither this node nor any descendant needs work
    if !node.cascade_dirty && !node.has_dirty_descendant {
        return;
    }

    if node.cascade_dirty {
        // Full re-cascade of this node (delegates to the existing cascade logic)
        // apply_cascade_inner handles this node AND recurses into all children,
        // which is correct because when a parent's hover state changes,
        // children may inherit different values or match descendant selectors differently.
        apply_cascade_inner(
            node, stylesheet, parent_style, root_font_px,
            ancestors, child_index, sibling_count, type_child_index, type_sibling_count,
            vw, vh, focused_box, keyboard_focus,
            inherited_vars, candidates_buf, counters, hover_chain,
        );
        return;
    }

    // has_dirty_descendant only — don't re-cascade this node, just recurse into children
    let anc = AncestorInfo {
        tag: node.tag.clone(),
        attributes: node.attributes.clone(),
        child_index,
        sibling_count,
        type_child_index,
        type_sibling_count,
        node_id: node.node_id,
    };
    ancestors.push(anc);

    let parent_s = node.style.clone();
    let child_count = node.children.len();
    for i in 0..child_count {
        let child_tag = node.children[i].tag.clone();
        let mut t_idx = 0usize;
        let mut t_count = 0usize;
        for (j, sib) in node.children.iter().enumerate() {
            if sib.tag == child_tag {
                if j == i { t_idx = t_count; }
                t_count += 1;
            }
        }
        let child = &mut node.children[i];
        apply_cascade_incremental_walk(
            child, stylesheet, Some(&parent_s), root_font_px,
            ancestors, i, child_count, t_idx, t_count,
            vw, vh, focused_box, keyboard_focus,
            inherited_vars, candidates_buf, counters,
            hover_chain,
        );
    }

    ancestors.pop();
}

/// Build the set of element pointers from root to the hovered element (hover chain).
/// Returns empty set if target is null or not found in the tree.
pub fn build_hover_chain(root: &crate::types::HtmlBox, target: u32) -> std::collections::HashSet<u32> {
    if target == 0 { return std::collections::HashSet::new(); }
    fn walk(node: &crate::types::HtmlBox, target: u32, path: &mut Vec<u32>) -> bool {
        path.push(node.node_id);
        if node.node_id != 0 && node.node_id == target { return true; }
        for child in &node.children {
            if walk(child, target, path) { return true; }
        }
        // Also search shadow tree
        if let Some(ref sr) = node.shadow_root {
            for child in &sr.children {
                if walk(child, target, path) { return true; }
            }
        }
        path.pop();
        false
    }
    let mut path = Vec::new();
    walk(root, target, &mut path);
    path.into_iter().collect()
}

/// Fast hover style swap — avoids full re-cascade on hover-only changes.
///
/// Walks the tree and swaps `style` ↔ `hover_style` for elements whose hover
/// state has changed.  Also creates/removes positioned `::before`/`::after`
/// pseudo-element children as needed.
///
/// Returns `true` if any style was changed (caller should re-layout).
pub fn swap_hover_state(
    root: &mut crate::types::HtmlBox,
    hover_chain: &std::collections::HashSet<u32>,
) -> bool {
    swap_hover_inner(root, hover_chain, false)
}

fn swap_hover_inner(
    node: &mut crate::types::HtmlBox,
    hover_chain: &std::collections::HashSet<u32>,
    ancestor_in_chain: bool,
) -> bool {
    // Skip synthetic pseudo-element children — their style is set by their parent
    if node.tag == "::before" || node.tag == "::after" { return false; }

    let self_in_chain = node.node_id != 0 && hover_chain.contains(&node.node_id);
    let in_hover = ancestor_in_chain || self_in_chain;
    let mut changed = false;

    // Swap style ↔ hover_style when the hover state differs from the current applied state
    if node.style.hover_style.is_some() {
        let should_hover = in_hover;
        if should_hover != node.hover_applied {
            // Swap: style becomes the other variant, hover_style stores the current
            let mut other = node.style.hover_style.take().unwrap();
            // Preserve hover_style/active_style/visited_style from the base side
            let hs_backup = node.style.hover_style.take(); // already None after take above
            let as_backup = node.style.active_style.take();
            let vs_backup = node.style.visited_style.take();
            // Preserve before/after pseudo styles from the incoming variant
            // (the other style may have different before_style/before_content)
            let cur_before_style = node.style.before_style.take();
            let cur_before_content = std::mem::take(&mut node.style.before_content);
            let cur_after_style = node.style.after_style.take();
            let cur_after_content = std::mem::take(&mut node.style.after_content);

            let cur_style = std::mem::replace(&mut node.style, *other);
            // Store the old style as the new hover_style (for swapping back)
            let mut stored = Box::new(cur_style);
            stored.hover_style = None;
            stored.active_style = None;
            stored.visited_style = None;
            // Preserve the old before/after in the stored style
            stored.before_style = cur_before_style;
            stored.before_content = cur_before_content;
            stored.after_style = cur_after_style;
            stored.after_content = cur_after_content;

            node.style.hover_style = Some(stored);
            node.style.active_style = as_backup;
            node.style.visited_style = vs_backup;
            node.hover_applied = should_hover;
            changed = true;

            // Handle ::before/::after pseudo-element creation/removal
            let is_grid_or_flex = matches!(node.style.display,
                crate::types::Display::Grid | crate::types::Display::InlineGrid
                | crate::types::Display::Flex | crate::types::Display::InlineFlex);
            let before_is_positioned = node.style.before_style.as_ref().map_or(false, |ps|
                matches!(ps.position, crate::types::Position::Absolute | crate::types::Position::Fixed));
            if (is_grid_or_flex && !node.style.before_content.is_empty())
                || (before_is_positioned && node.style.before_style.is_some())
            {
                let existing = node.children.iter().position(|c| c.tag == "::before");
                let mut pseudo_box = crate::types::HtmlBox::new("::before");
                pseudo_box.text = node.style.before_content.clone();
                if let Some(ref ps) = node.style.before_style {
                    pseudo_box.style = *ps.clone();
                }
                if let Some(idx) = existing {
                    node.children[idx] = pseudo_box;
                } else {
                    node.children.insert(0, pseudo_box);
                }
                node.style.before_content = String::new();
            } else if let Some(idx) = node.children.iter().position(|c| c.tag == "::before") {
                node.children.remove(idx);
            }

            let after_is_positioned = node.style.after_style.as_ref().map_or(false, |ps|
                matches!(ps.position, crate::types::Position::Absolute | crate::types::Position::Fixed));
            if (is_grid_or_flex && !node.style.after_content.is_empty())
                || (after_is_positioned && node.style.after_style.is_some())
            {
                let existing = node.children.iter().position(|c| c.tag == "::after");
                let mut pseudo_box = crate::types::HtmlBox::new("::after");
                pseudo_box.text = node.style.after_content.clone();
                if let Some(ref ps) = node.style.after_style {
                    pseudo_box.style = *ps.clone();
                }
                if let Some(idx) = existing {
                    node.children[idx] = pseudo_box;
                } else {
                    node.children.push(pseudo_box);
                }
                node.style.after_content = String::new();
            } else if let Some(idx) = node.children.iter().position(|c| c.tag == "::after") {
                node.children.remove(idx);
            }
        }
    }

    for child in &mut node.children {
        changed |= swap_hover_inner(child, hover_chain, in_hover);
    }

    changed
}

#[allow(clippy::too_many_arguments)]
/// Maximum DOM depth before we stop recursing to avoid stack overflow.
/// 400 levels is more than any well-formed page needs (most pages are < 50 deep).
const MAX_CASCADE_DEPTH: usize = 400;

fn apply_cascade_inner(
    root: &mut crate::types::HtmlBox,
    stylesheet: &Stylesheet,
    parent_style: Option<&ComputedStyle>,
    root_font_px: f32,
    // Mutable: we push this element's info before recursing and pop after.
    // One Vec is reused for the entire tree — no per-node heap allocation.
    ancestors: &mut Vec<AncestorInfo>,
    child_index: usize,
    sibling_count: usize,
    type_child_index: usize,
    type_sibling_count: usize,
    vw: f32,
    vh: f32,
    focused_box: u32,
    keyboard_focus: bool,
    inherited_vars: &HashMap<String, String>,
    candidates_buf: &mut Vec<usize>,
    counters: &mut HashMap<String, Vec<i32>>,
    hover_chain: &std::collections::HashSet<u32>,
) {
    // Guard against stack overflow on deeply nested DOMs.
    if ancestors.len() >= MAX_CASCADE_DEPTH {
        // Just inherit from parent and stop — the page may render slightly wrong
        // at extreme depth, but won't crash.
        if let Some(p) = parent_style {
            root.style.inherit_from(p);
        }
        return;
    }

    // Text nodes are not elements — they inherit from their parent but
    // must never match CSS selectors (including `*`).
    if root.tag == "#text" {
        if let Some(p) = parent_style {
            root.style.inherit_from(p);
        }
        return;
    }

    // Synthetic ::before/::after children already have their style set.
    // Skip the cascade for them — just recurse into their children (if any) and return.
    if root.tag == "::before" || root.tag == "::after" {
        // Still inherit inheritable properties from parent
        if let Some(p) = parent_style {
            let saved_display = root.style.display;
            root.style.inherit_from(p);
            root.style.display = saved_display; // preserve blockified display
        }
        return;
    }

    // Start with default style and inherit from parent
    let mut style = ComputedStyle::default();
    if let Some(p) = parent_style {
        style.inherit_from(p);
    }

    // Apply presentational HTML attributes (specificity 0 — before author rules)
    let attrs = root.attributes.clone();
    for (attr, val) in &attrs {
        match attr.as_str() {
            "align" => match val.as_str() {
                "center" => apply_property(&mut style, "text-align", "center"),
                "right"  => apply_property(&mut style, "text-align", "right"),
                "left"   => apply_property(&mut style, "text-align", "left"),
                _ => {}
            },
            "valign"  => apply_property(&mut style, "vertical-align", val),
            "bgcolor" => apply_property(&mut style, "background-color", val),
            "color" | "text" => apply_property(&mut style, "color", val),
            "face"  => apply_property(&mut style, "font-family", val),
            "size"  => {
                // HTML <font size="1..7"> maps to absolute px sizes
                let px: f32 = match val.trim() {
                    "1" => 10.0, "2" => 13.0, "3" => 16.0,
                    "4" => 18.0, "5" => 24.0, "6" => 32.0, "7" => 48.0,
                    v   => v.parse::<f32>().unwrap_or(16.0),
                };
                apply_property(&mut style, "font-size", &format!("{}px", px));
            }
            "width" => {
                if val.ends_with('%') {
                    apply_property(&mut style, "width", val);
                } else if val.parse::<f32>().is_ok() {
                    apply_property(&mut style, "width", &format!("{}px", val));
                }
            }
            "height" => {
                if val.ends_with('%') {
                    apply_property(&mut style, "height", val);
                } else if val.parse::<f32>().is_ok() {
                    apply_property(&mut style, "height", &format!("{}px", val));
                }
            }
            "border" if root.tag == "table" => {
                // HTML border attr on <table>: sets a solid frame and collapses borders
                // so the table frame and cell borders merge into a single grid (like browsers).
                if let Ok(w) = val.parse::<f32>() {
                    if w > 0.0 {
                        apply_property(&mut style, "border", &format!("{}px solid", w));
                        apply_property(&mut style, "border-collapse", "collapse");
                    } else {
                        apply_property(&mut style, "border", "0px solid transparent");
                    }
                }
            }
            "cellspacing" => {
                // Maps to CSS border-spacing.
                if let Ok(n) = val.parse::<f32>() {
                    apply_property(&mut style, "border-spacing", &format!("{}px", n));
                } else if val.ends_with("px") {
                    apply_property(&mut style, "border-spacing", val);
                }
            }
            "cellpadding" => {
                apply_property(&mut style, "cellpadding", val);
            }
            "dir" => match val.to_ascii_lowercase().as_str() {
                "rtl" => apply_property(&mut style, "direction", "rtl"),
                _     => apply_property(&mut style, "direction", "ltr"),
            },
            _ => {}
        }
    }

    // HTML: td/th inside a table with border="N" (N>0) get a 1px inset border,
    // matching browser UA behaviour. Applied at presentational-attribute specificity
    // so author CSS can override.
    if matches!(root.tag.as_str(), "td" | "th") {
        let has_table_border = ancestors.iter().rev().any(|a| {
            a.tag == "table" && a.attributes.get("border")
                .and_then(|v| v.parse::<f32>().ok())
                .map_or(false, |n| n > 0.0)
        });
        if has_table_border {
            apply_property(&mut style, "border", "1px solid");
        }
    }

    // Build MatchContext for this element
    let match_ctx = MatchContext {
        focused_box,
        keyboard_focus,
        type_child_index,
        type_sibling_count,
        html_box: Some(root),
        hover_chain,
        element_id: root.node_id,
    };

    // Apply UA / author stylesheet rules (after presentational attrs, before inline style)
    // Store (specificity, rule_index) — avoid cloning declaration HashMaps per match.
    let mut matched:           Vec<(u32, usize)> = Vec::new();
    let mut hover_matched:   Vec<(u32, usize)> = Vec::new();
    let mut active_matched:  Vec<(u32, usize)> = Vec::new();
    let mut visited_matched: Vec<(u32, usize)> = Vec::new();
    let mut before_matched:    Vec<(u32, usize)> = Vec::new();
    let mut after_matched:     Vec<(u32, usize)> = Vec::new();
    let mut selection_matched: Vec<(u32, usize)> = Vec::new();
    let mut marker_matched:    Vec<(u32, usize)> = Vec::new();

    // Use selector index to narrow down candidate rules instead of scanning all rules.
    let tag = &root.tag;
    let id = root.attributes.get("id").map(|s| s.as_str());
    let class_attr = root.attributes.get("class").cloned().unwrap_or_default();
    let classes: Vec<&str> = class_attr.split_whitespace().collect();
    stylesheet.candidate_rules(tag, id, &classes, candidates_buf);

    for &rule_idx in candidates_buf.iter() {
        let rule = &stylesheet.rules[rule_idx];
        // Skip rules whose @media condition doesn't match the viewport
        if !rule.media_condition.is_empty() && !evaluate_media(&rule.media_condition, vw, vh) {
            continue;
        }
        // Container rules require layout context — applied in a post-layout pass.
        if !rule.container_condition.is_empty() { continue; }
        for sel in &rule.selectors {
            // Use pre-computed per-selector state flags (no scanning needed).
            let has_hover   = sel.has_hover;
            let has_active  = sel.has_active;
            let has_visited = sel.has_visited;

            if (has_hover || has_active || has_visited) && rule.pseudo_element == PseudoElement::None {
                // Strip state pseudo-classes and match base selector.
                let base_parts: Vec<SelectorPart> = sel.parts.iter()
                    .filter(|p| !matches!(p, SelectorPart::PseudoClass(n)
                        if matches!(n.as_str(), "hover" | "active" | "visited")))
                    .cloned()
                    .collect();
                let base_sel = CssSelector::new(base_parts);
                if base_sel.matches_with_ancestors_ctx(root, child_index, sibling_count, ancestors, &match_ctx) {
                    if has_hover   { hover_matched.push((rule.specificity, rule_idx)); }
                    if has_active  { active_matched.push((rule.specificity, rule_idx)); }
                    if has_visited { visited_matched.push((rule.specificity, rule_idx)); }
                    // When hover chain is active, also match the FULL selector
                    // (with :hover intact). If it matches, apply the rule as a
                    // normal rule so it affects layout (e.g. display:block on hover).
                    if has_hover && !hover_chain.is_empty() {
                        if sel.matches_with_ancestors_ctx(root, child_index, sibling_count, ancestors, &match_ctx) {
                            matched.push((rule.specificity, rule_idx));
                        }
                    }
                    break;
                }
                continue;
            }
            if sel.matches_with_ancestors_ctx(root, child_index, sibling_count, ancestors, &match_ctx) {
                match rule.pseudo_element {
                    PseudoElement::Before     => before_matched.push((rule.specificity, rule_idx)),
                    PseudoElement::After      => after_matched.push((rule.specificity, rule_idx)),
                    PseudoElement::Selection  => selection_matched.push((rule.specificity, rule_idx)),
                    PseudoElement::Marker     => marker_matched.push((rule.specificity, rule_idx)),
                    PseudoElement::None       => matched.push((rule.specificity, rule_idx)),
                    PseudoElement::Ignored    => {}
                }
                break;
            }
        }
    }
    matched.sort_by_key(|(sp, _)| *sp);
    // Build variable scope: inherited from parent + any --custom-properties from matched rules.
    // Only clone the map when new custom properties are actually defined — most elements
    // don't define any, so we avoid O(vars) cloning at every node.
    let has_new_vars = matched.iter().any(|(_, ri)| {
        stylesheet.rules[*ri].declarations.keys().any(|p| p.starts_with("--"))
        || stylesheet.rules[*ri].important_declarations.keys().any(|p| p.starts_with("--"))
    });
    // Also check inline style for custom properties — these must be available
    // during var() resolution of stylesheet rules on the same element.
    let inline_decls = root.attributes.get("style").cloned()
        .map(|s| parse_declarations_important(&s));
    let has_inline_vars = inline_decls.as_ref()
        .map(|(n, _)| n.keys().any(|p| p.starts_with("--")))
        .unwrap_or(false);

    let mut local_vars_owned = HashMap::new();
    let mut local_vars: &HashMap<String, String> = if has_new_vars || has_inline_vars {
        let mut vars = inherited_vars.clone();
        for &(_, ri) in &matched {
            for (prop, val) in &stylesheet.rules[ri].declarations {
                if prop.starts_with("--") {
                    vars.insert(prop.clone(), val.clone());
                }
            }
            for (prop, val) in &stylesheet.rules[ri].important_declarations {
                if prop.starts_with("--") {
                    vars.insert(prop.clone(), val.clone());
                }
            }
        }
        // Inline custom properties override stylesheet ones (higher specificity)
        if let Some((ref n, _)) = inline_decls {
            for (prop, val) in n {
                if prop.starts_with("--") {
                    vars.insert(prop.clone(), val.clone());
                }
            }
        }
        pre_resolve_variables(&mut vars);
        local_vars_owned = vars;
        &local_vars_owned
    } else {
        inherited_vars
    };
    // Track properties whose highest-specificity declaration is `inherit`.
    // After all rules are applied, these properties are reset to the parent's value.
    let mut inherit_props: HashSet<String> = HashSet::new();
    let has_vars = !local_vars.is_empty();
    for &(_, ri) in &matched {
        // Fast path: use pre-compiled declarations (PropertyId dispatch, no string matching).
        // Only fall back to raw declarations when var() resolution is needed.
        let rule = &stylesheet.rules[ri];
        if has_vars && rule.has_var_refs {
            // Slow path: var() references need string-based resolution
            for (prop, val) in &rule.declarations {
                if prop.starts_with("--") { continue; }
                let resolved = resolve_var_references(val, &local_vars);
                if resolved.trim().is_empty() && val.contains("var(") { continue; }
                if resolved.trim() == "inherit" {
                    inherit_props.insert(prop.to_string());
                } else {
                    inherit_props.remove(prop.as_str());
                    apply_property(&mut style, prop, &resolved);
                }
            }
        } else {
            // Fast path: no var() — use compiled declarations directly
            for &(id, ref val) in &rule.compiled_decls {
                let v = val.trim();
                if v == "inherit" {
                    let name = property_defs::get(id).name;
                    inherit_props.insert(name.to_string());
                } else {
                    apply_property_by_id(&mut style, id, v);
                }
            }
        }
    }
    // Re-apply parent values for properties whose winning declaration was `inherit`.
    if !inherit_props.is_empty() {
        if let Some(p) = parent_style {
            for prop in &inherit_props {
                copy_property_from_parent(&mut style, p, prop);
            }
        }
    }
    // Hover style — clone the base style and overlay all matched hover declarations.
    if !hover_matched.is_empty() {
        hover_matched.sort_by_key(|(sp, _)| *sp);
        let mut hs = style.clone();
        for &(_, ri) in &hover_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_decls {
                let resolved = resolve_var_references(val, &local_vars);
                if resolved.trim().is_empty() && val.contains("var(") { continue; }
                apply_property_by_id(&mut hs, id, &resolved);
            }
        }
        for &(_, ri) in &hover_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
                let resolved = resolve_var_references(val, &local_vars);
                if resolved.trim().is_empty() && val.contains("var(") { continue; }
                apply_property_by_id(&mut hs, id, &resolved);
            }
        }
        // Prevent infinite nesting: state styles don't carry their own state overrides.
        hs.hover_style = None; hs.active_style = None; hs.visited_style = None;
        style.hover_style = Some(Box::new(hs));
    }
    // Active style — clone the base style and overlay all matched active declarations.
    if !active_matched.is_empty() {
        active_matched.sort_by_key(|(sp, _)| *sp);
        let mut as_ = style.clone();
        for &(_, ri) in &active_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_decls {
                let resolved = resolve_var_references(val, &local_vars);
                if resolved.trim().is_empty() && val.contains("var(") { continue; }
                apply_property_by_id(&mut as_, id, &resolved);
            }
        }
        for &(_, ri) in &active_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
                let resolved = resolve_var_references(val, &local_vars);
                if resolved.trim().is_empty() && val.contains("var(") { continue; }
                apply_property_by_id(&mut as_, id, &resolved);
            }
        }
        as_.hover_style = None; as_.active_style = None; as_.visited_style = None;
        style.active_style = Some(Box::new(as_));
    }
    // Visited style — clone the base style and overlay all matched visited declarations.
    if !visited_matched.is_empty() {
        visited_matched.sort_by_key(|(sp, _)| *sp);
        let mut vs = style.clone();
        for &(_, ri) in &visited_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_decls {
                let resolved = resolve_var_references(val, &local_vars);
                if resolved.trim().is_empty() && val.contains("var(") { continue; }
                apply_property_by_id(&mut vs, id, &resolved);
            }
        }
        for &(_, ri) in &visited_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
                let resolved = resolve_var_references(val, &local_vars);
                if resolved.trim().is_empty() && val.contains("var(") { continue; }
                apply_property_by_id(&mut vs, id, &resolved);
            }
        }
        vs.hover_style = None; vs.active_style = None; vs.visited_style = None;
        style.visited_style = Some(Box::new(vs));
    }

    // Apply inline style attribute (normal declarations).
    // Custom properties were already merged into local_vars above.
    // Also collect inline hover-* properties for building hover_style.
    let mut inline_hover_props: Vec<(String, String)> = Vec::new();
    let (_inline_normal, inline_important) = if let Some((n, i)) = inline_decls {
        for (prop, val) in &n {
            if prop.starts_with("--") { continue; }
            // Inline hover-* properties: hover-background-color → background-color on hover
            if let Some(real_prop) = prop.strip_prefix("hover-") {
                let resolved = resolve_var_references(val, local_vars);
                inline_hover_props.push((real_prop.to_string(), resolved));
                continue;
            }
            let resolved = resolve_var_references(val, local_vars);
            if resolved.trim() == "inherit" {
                if let Some(p) = parent_style { copy_property_from_parent(&mut style, p, prop); }
            } else {
                apply_property(&mut style, prop, &resolved);
            }
        }
        (n, i)
    } else {
        (HashMap::new(), HashMap::new())
    };
    // Merge inline hover-* properties into hover_style
    if !inline_hover_props.is_empty() {
        let mut hs = if let Some(existing) = style.hover_style.take() {
            *existing
        } else {
            style.clone()
        };
        for (prop, val) in &inline_hover_props {
            apply_property(&mut hs, prop, val);
        }
        hs.hover_style = None; hs.active_style = None; hs.visited_style = None;
        style.hover_style = Some(Box::new(hs));
    }

    // Apply !important stylesheet declarations — these override inline styles
    for &(_, ri) in &matched {
        for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
            let resolved = resolve_var_references(val, &local_vars);
            if resolved.trim().is_empty() && val.contains("var(") { continue; }
            apply_property_by_id(&mut style, id, &resolved);
        }
    }

    // Apply inline style !important declarations — highest priority
    for (prop, val) in &inline_important {
        let resolved = resolve_var_references(val, &local_vars);
        if resolved.trim() == "inherit" {
            if let Some(p) = parent_style { copy_property_from_parent(&mut style, p, prop); }
        } else {
            apply_property(&mut style, prop, &resolved);
        }
    }

    // Re-apply table layout HTML attributes after CSS rules so UA/author stylesheets
    // cannot silently override them (e.g. UA "border-spacing: 2px" must not win over
    // cellspacing="0").  These are still below inline style priority.
    if root.tag == "table" {
        if let Some(v) = root.attributes.get("cellspacing").cloned() {
            if let Ok(n) = v.parse::<f32>() {
                apply_property(&mut style, "border-spacing", &format!("{}px", n));
            }
        }
        if let Some(v) = root.attributes.get("cellpadding").cloned() {
            apply_property(&mut style, "cellpadding", &v);
        }
        if let Some(v) = root.attributes.get("border").cloned() {
            if let Ok(n) = v.parse::<f32>() {
                if n > 0.0 {
                    apply_property(&mut style, "border-collapse", "collapse");
                }
            }
        }
    }

    // Capture href from attributes (non-standard CSS, but useful for our editor)
    if let Some(href) = root.attributes.get("href") {
        style.href = href.clone();
    }

    // Resolve relative font size to absolute Px for inheritance parity
    let parent_font_px = parent_style.map(|p| p.font_size_px(root_font_px, root_font_px)).unwrap_or(root_font_px);
    let font_px = style.font_size_px(parent_font_px, root_font_px);
    style.font_size = CssLength::Px(font_px);

    // If this is the root element (<html>), its computed font-size becomes the
    // new root font-size used for `rem` resolution in all descendants.
    // e.g. `html { font-size: 62.5% }` → 1rem = 10px instead of 16px.
    let root_font_px = if root.tag.eq_ignore_ascii_case("html") {
        font_px
    } else {
        root_font_px
    };

    // Preserve list_index: set by the HTML parser (ol counter), not by CSS.
    // The fresh ComputedStyle defaults list_index=0, so carry the old value forward.
    style.list_index = root.style.list_index;
    // Safety: block-level elements that end up as Inline despite no author CSS
    // setting display:inline should be forced to Block. Only apply when no
    // matched rule explicitly sets display (i.e., the Inline came from default).
    if matches!(style.display, Display::Inline) {
        let has_explicit_display = matched.iter().any(|&(_, ri)| {
            stylesheet.rules[ri].declarations.iter().any(|(k, _)| k == "display")
        });
        if !has_explicit_display {
            let should_be_block = matches!(root.tag.as_str(),
                "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                | "ul" | "ol" | "dl" | "dt" | "dd" | "pre" | "blockquote" | "hr"
                | "section" | "article" | "aside" | "nav" | "header" | "footer" | "main"
                | "address" | "figure" | "figcaption" | "details" | "center"
                | "form" | "fieldset" | "legend" | "hgroup" | "search");
            if should_be_block {
                style.display = Display::Block;
            }
        }
    }
    root.style = style.clone();
    // Store matched CSS rules for inspector (only when enabled).
    if stylesheet.inspect_mode {
        root.matched_rules.clear();
        for &(sp, ri) in &matched {
            let rule = &stylesheet.rules[ri];
            root.matched_rules.push(crate::types::MatchedRule {
                selector: rule.original_selector.clone(),
                declarations: rule.declarations.iter()
                    .map(|(k, v)| (k.clone(), resolve_var_references(v, &local_vars)))
                    .collect(),
                specificity: sp,
                source: if ri < 50 { "ua".to_string() }
                        else { rule.media_condition.clone() },
            });
        }
    }
    // Mark dirty so the layout subtree pruning (in layout_box_with_fc) knows to
    // re-layout this element.  Cleared by the individual layout algorithms after
    // they have computed the final geometry.
    root.layout.layout_dirty = true;

    // <form> inside table elements: browsers treat it as transparent (display:contents)
    // so it doesn't break table row grouping. Check if any ancestor is a table element.
    if root.tag == "form" {
        let in_table = ancestors.iter().any(|a|
            matches!(a.tag.as_str(), "table" | "thead" | "tbody" | "tfoot" | "tr"));
        if in_table {
            root.style.display = Display::Contents;
        }
    }

    // Build full ComputedStyle for ::before / ::after pseudo-elements.
    // Each inherits from the element's computed style, then has its own declarations applied.
    let build_pseudo_style = |matched: &mut Vec<(u32, usize)>,
                               base: &ComputedStyle,
                               vars: &HashMap<String, String>,
                               rules: &[CssRule]|
     -> Option<(String, Box<ComputedStyle>)> {
        if matched.is_empty() { return None; }
        matched.sort_by_key(|(sp, _)| *sp);
        let mut ps = base.clone();
        ps.before_style = None;   // pseudo-elements don't nest
        ps.after_style  = None;
        ps.before_content = String::new();
        ps.after_content  = String::new();
        let mut content = String::new();
        for &(_, ri) in matched.iter() {
            for (prop, val) in &rules[ri].declarations {
                let resolved = resolve_var_references(val, vars);
                if prop == "content" {
                    content = resolve_content_value(&resolved);
                } else {
                    apply_property(&mut ps, prop, &resolved);
                }
            }
        }
        for &(_, ri) in matched.iter() {
            for (prop, val) in &rules[ri].important_declarations {
                let resolved = resolve_var_references(val, vars);
                if prop == "content" {
                    content = resolve_content_value(&resolved);
                } else {
                    apply_property(&mut ps, prop, &resolved);
                }
            }
        }
        Some((content, Box::new(ps)))
    };

    // ── CSS counters: reset, increment, then resolve counter() in content ──
    // Track which counters were reset at this level so we can pop them later.
    let mut counters_pushed: Vec<String> = Vec::new();
    for (name, val) in &root.style.counter_reset {
        counters.entry(name.clone()).or_insert_with(Vec::new).push(*val);
        counters_pushed.push(name.clone());
    }
    // `ol` implicitly resets the `list-item` counter
    if root.tag == "ol" && root.style.counter_reset.is_empty() {
        counters.entry("list-item".to_string()).or_insert_with(Vec::new).push(0);
        counters_pushed.push("list-item".to_string());
    }
    for (name, val) in &root.style.counter_increment {
        if let Some(stack) = counters.get_mut(name) {
            if let Some(top) = stack.last_mut() {
                *top += val;
            }
        }
    }
    // `li` implicitly increments the `list-item` counter
    if root.tag == "li" && root.style.counter_increment.is_empty() {
        if let Some(stack) = counters.get_mut("list-item") {
            if let Some(top) = stack.last_mut() {
                *top += 1;
            }
        }
    }

    if let Some((txt, ps)) = build_pseudo_style(&mut before_matched, &root.style, &local_vars, &stylesheet.rules) {
        // ::before may carry counter-increment/counter-reset — apply before resolving content
        for (name, val) in &ps.counter_reset {
            counters.entry(name.clone()).or_insert_with(Vec::new).push(*val);
            counters_pushed.push(name.clone());
        }
        for (name, val) in &ps.counter_increment {
            if let Some(stack) = counters.get_mut(name) {
                if let Some(top) = stack.last_mut() {
                    *top += val;
                }
            }
        }
        root.style.before_content = resolve_counters_in_content(&txt, counters);
        root.style.before_style   = Some(ps);
    }
    if let Some((txt, ps)) = build_pseudo_style(&mut after_matched, &root.style, &local_vars, &stylesheet.rules) {
        root.style.after_content = resolve_counters_in_content(&txt, counters);
        root.style.after_style   = Some(ps);
    }
    if let Some((_, ps)) = build_pseudo_style(&mut selection_matched, &root.style, &local_vars, &stylesheet.rules) {
        root.style.selection_style = Some(ps);
    }
    if let Some((_, ps)) = build_pseudo_style(&mut marker_matched, &root.style, &local_vars, &stylesheet.rules) {
        root.style.marker_style = Some(ps);
    }

    // For grid/flex containers, ::before/::after generate actual child boxes
    // (they become grid/flex items). Insert synthetic children.
    let is_grid_or_flex = matches!(root.style.display,
        Display::Grid | Display::InlineGrid | Display::Flex | Display::InlineFlex);
    // Create ::before child element when the pseudo-element needs to be a real box:
    // - Grid/flex containers always create child boxes for pseudo-elements (needs content)
    // - Positioned pseudo-elements (position:absolute/fixed) need real boxes even with
    //   empty content (used for decorative backgrounds like USPS nav blue bar)
    // - Block-level pseudo-elements need real boxes
    let before_is_positioned = root.style.before_style.as_ref().map_or(false, |ps|
        matches!(ps.position, Position::Absolute | Position::Fixed));
    if (is_grid_or_flex && !root.style.before_content.is_empty())
        || (before_is_positioned && root.style.before_style.is_some())
    {
        // Check if a ::before child already exists (from a prior cascade pass)
        let existing = root.children.iter().position(|c| c.tag == "::before");
        let mut pseudo_box = {
                let mut b = crate::types::HtmlBox::new("::before");
                b.text = root.style.before_content.clone();
                b
            };
        pseudo_box.tag = "::before".to_string();
        if let Some(ref ps) = root.style.before_style {
            pseudo_box.style = *ps.clone();
        }
        // Blockify inline pseudo-elements in grid/flex containers (not positioned ones)
        if is_grid_or_flex && !pseudo_box.style.is_positioned()
            && matches!(pseudo_box.style.display, Display::Inline) {
            pseudo_box.style.display = Display::Block;
        }
        if let Some(idx) = existing {
            root.children[idx] = pseudo_box; // update existing
        } else {
            root.children.insert(0, pseudo_box);
        }
        // Clear before_content so it doesn't also render inline
        root.style.before_content = String::new();
    } else {
        // Remove stale ::before child from a prior cascade pass (e.g. hover-out)
        if let Some(idx) = root.children.iter().position(|c| c.tag == "::before") {
            root.children.remove(idx);
        }
    }
    let after_is_positioned = root.style.after_style.as_ref().map_or(false, |ps|
        matches!(ps.position, Position::Absolute | Position::Fixed));
    if (is_grid_or_flex && !root.style.after_content.is_empty())
        || (after_is_positioned && root.style.after_style.is_some())
    {
        // Check if an ::after child already exists (from a prior cascade pass)
        let existing = root.children.iter().position(|c| c.tag == "::after");
        let mut pseudo_box = {
                let mut b = crate::types::HtmlBox::new("::after");
                b.text = root.style.after_content.clone();
                b
            };
        pseudo_box.tag = "::after".to_string();
        if let Some(ref ps) = root.style.after_style {
            pseudo_box.style = *ps.clone();
        }
        if is_grid_or_flex && !pseudo_box.style.is_positioned()
            && matches!(pseudo_box.style.display, Display::Inline) {
            pseudo_box.style.display = Display::Block;
        }
        if let Some(idx) = existing {
            root.children[idx] = pseudo_box; // update existing
        } else {
            root.children.push(pseudo_box);
        }
        root.style.after_content = String::new();
    } else {
        // Remove stale ::after child from a prior cascade pass (e.g. hover-out)
        if let Some(idx) = root.children.iter().position(|c| c.tag == "::after") {
            root.children.remove(idx);
        }
    }

    // Push this element's info so children can see it as an ancestor.
    // Popped after children are processed — the Vec is reused for the whole tree.
    ancestors.push(AncestorInfo {
        tag:                root.tag.clone(),
        attributes:         root.attributes.clone(),
        child_index,
        sibling_count,
        type_child_index,
        type_sibling_count,
        node_id:            root.node_id,
    });

    // Helper: cascade a list of children with a given stylesheet
    fn cascade_children(
        children: &mut [crate::types::HtmlBox],
        stylesheet: &Stylesheet,
        parent_style: &ComputedStyle,
        root_font_px: f32,
        ancestors: &mut Vec<AncestorInfo>,
        vw: f32, vh: f32,
        focused_box: u32,
        keyboard_focus: bool,
        inherited_vars: &HashMap<String, String>,
        candidates_buf: &mut Vec<usize>,
        counters: &mut HashMap<String, Vec<i32>>,
        hover_chain: &std::collections::HashSet<u32>,
    ) {
        let n_children = children.len();
        if n_children == 0 { return; }
        let child_tags: Vec<String> = children.iter().map(|c| c.tag.to_ascii_lowercase()).collect();
        let mut type_running: HashMap<&str, usize> = HashMap::new();
        let type_counts: Vec<usize> = child_tags.iter().map(|tag| {
            let slot = type_running.entry(tag.as_str()).or_insert(0);
            let idx  = *slot; *slot += 1; idx
        }).collect();
        let type_totals: Vec<usize> = child_tags.iter().map(|tag| {
            *type_running.get(tag.as_str()).unwrap_or(&0)
        }).collect();
        let n_elem_children = children.iter().filter(|c| c.tag != "#text").count();
        let mut elem_pos = 0usize;
        let elem_indices: Vec<usize> = children.iter().map(|c| {
            if c.tag == "#text" { 0 } else { let p = elem_pos; elem_pos += 1; p }
        }).collect();
        for (i, child) in children.iter_mut().enumerate() {
            let (ci, ns) = if child.tag == "#text" {
                (i, n_children)
            } else {
                (elem_indices[i], n_elem_children)
            };
            apply_cascade_inner(
                child, stylesheet, Some(parent_style), root_font_px,
                ancestors, ci, ns,
                type_counts[i], type_totals[i],
                vw, vh, focused_box, keyboard_focus,
                inherited_vars, candidates_buf, counters,
                hover_chain,
            );
        }
    }

    // Shadow DOM: cascade shadow children with the shadow's scoped stylesheet,
    // and also cascade light DOM children with the document stylesheet.
    // CSS custom properties cross the shadow boundary via inherited_vars.
    if root.shadow_root.is_some() {
        // Take shadow root temporarily to satisfy borrow checker
        let mut sr = root.shadow_root.take().unwrap();
        sr.stylesheet.rebuild_index();
        cascade_children(
            &mut sr.children, &sr.stylesheet, &style, root_font_px,
            ancestors, vw, vh, focused_box, keyboard_focus,
            &local_vars, candidates_buf, counters, hover_chain,
        );
        root.shadow_root = Some(sr);
        // Also cascade light DOM children (they need document styles for ::slotted)
        cascade_children(
            &mut root.children, stylesheet, &style, root_font_px,
            ancestors, vw, vh, focused_box, keyboard_focus,
            &local_vars, candidates_buf, counters, hover_chain,
        );
    } else {
        cascade_children(
            &mut root.children, stylesheet, &style, root_font_px,
            ancestors, vw, vh, focused_box, keyboard_focus,
            &local_vars, candidates_buf, counters, hover_chain,
        );
    }

    ancestors.pop();

    // Pop counters that were reset at this level
    for name in counters_pushed.iter().rev() {
        if let Some(stack) = counters.get_mut(name) {
            stack.pop();
            if stack.is_empty() { counters.remove(name); }
        }
    }
}

// ─── Parallel Cascade ────────────────────────────────────────────────────────

/// Work item for the parallel cascade: one element extracted from the tree.
struct CascadeWorkItem {
    /// Path from root to this node (indices into children arrays).
    node_path: Vec<usize>,
    tag: String,
    attributes: HashMap<String, String>,
    class_attr: String,
    id: Option<String>,
    ancestors: Vec<AncestorInfo>,
    child_index: usize,
    sibling_count: usize,
    type_child_index: usize,
    type_sibling_count: usize,
    node_id: u32,
}

/// Result of parallel selector matching for one element.
struct CascadeMatchResult {
    node_path: Vec<usize>,
    matched:          Vec<(u32, usize)>,
    hover_matched:    Vec<(u32, usize)>,
    active_matched:   Vec<(u32, usize)>,
    visited_matched:  Vec<(u32, usize)>,
    before_matched:   Vec<(u32, usize)>,
    after_matched:    Vec<(u32, usize)>,
    selection_matched: Vec<(u32, usize)>,
    marker_matched:   Vec<(u32, usize)>,
}

/// Pass 1: Flatten the DOM tree into a work list.
/// Each element gets its ancestor chain snapshot (needed for descendant selectors).
fn flatten_tree_for_cascade(
    node: &crate::types::HtmlBox,
    ancestors: &mut Vec<AncestorInfo>,
    path: &mut Vec<usize>,
    child_index: usize,
    sibling_count: usize,
    type_child_index: usize,
    type_sibling_count: usize,
    out: &mut Vec<CascadeWorkItem>,
) {
    if ancestors.len() >= MAX_CASCADE_DEPTH { return; }
    // Skip text nodes and pseudo-elements — they don't match CSS selectors.
    if node.tag == "#text" || node.tag == "::before" || node.tag == "::after" {
        return;
    }

    let class_attr = node.attributes.get("class").cloned().unwrap_or_default();
    let id = node.attributes.get("id").cloned();

    out.push(CascadeWorkItem {
        node_path: path.clone(),
        tag: node.tag.clone(),
        attributes: node.attributes.clone(),
        class_attr,
        id,
        ancestors: ancestors.clone(),
        child_index,
        sibling_count,
        type_child_index,
        type_sibling_count,
        node_id: node.node_id,
    });

    // Push this element as ancestor for children.
    ancestors.push(AncestorInfo {
        tag:                node.tag.clone(),
        attributes:         node.attributes.clone(),
        child_index,
        sibling_count,
        type_child_index,
        type_sibling_count,
        node_id:            node.node_id,
    });

    // Compute per-child type indices.
    let n_children = node.children.len();
    if n_children > 0 {
        let child_tags: Vec<String> = node.children.iter().map(|c| c.tag.to_ascii_lowercase()).collect();
        let mut type_running: HashMap<&str, usize> = HashMap::new();
        let type_counts: Vec<usize> = child_tags.iter().map(|tag| {
            let slot = type_running.entry(tag.as_str()).or_insert(0);
            let idx = *slot; *slot += 1; idx
        }).collect();
        let type_totals: Vec<usize> = child_tags.iter().map(|tag| {
            *type_running.get(tag.as_str()).unwrap_or(&0)
        }).collect();
        let n_elem_children = node.children.iter().filter(|c| c.tag != "#text").count();
        let mut elem_pos = 0usize;
        let elem_indices: Vec<usize> = node.children.iter().map(|c| {
            if c.tag == "#text" { 0 } else { let p = elem_pos; elem_pos += 1; p }
        }).collect();

        for (i, child) in node.children.iter().enumerate() {
            let (ci, ns) = if child.tag == "#text" {
                (i, n_children)
            } else {
                (elem_indices[i], n_elem_children)
            };
            path.push(i);
            flatten_tree_for_cascade(
                child, ancestors, path, ci, ns,
                type_counts[i], type_totals[i], out,
            );
            path.pop();
        }
    }

    ancestors.pop();
}

/// Pass 2: Run selector matching in parallel for all work items.
fn parallel_selector_match(
    work_items: &[CascadeWorkItem],
    stylesheet: &Stylesheet,
    vw: f32,
    vh: f32,
    focused_box: u32,
    keyboard_focus: bool,
    hover_chain: &std::collections::HashSet<u32>,
) -> Vec<CascadeMatchResult> {
    work_items.par_iter().map(|item| {
        let mut candidates_buf: Vec<usize> = Vec::new();
        let classes: Vec<&str> = item.class_attr.split_whitespace().collect();
        stylesheet.candidate_rules(&item.tag, item.id.as_deref(), &classes, &mut candidates_buf);

        let match_ctx = MatchContext {
            focused_box,
            keyboard_focus,
            type_child_index: item.type_child_index,
            type_sibling_count: item.type_sibling_count,
            html_box: None, // Not available in parallel pass; :has()/:focus-within degrade gracefully
            hover_chain,
            element_id: item.node_id,
        };

        let mut matched:           Vec<(u32, usize)> = Vec::new();
        let mut hover_matched:     Vec<(u32, usize)> = Vec::new();
        let mut active_matched:    Vec<(u32, usize)> = Vec::new();
        let mut visited_matched:   Vec<(u32, usize)> = Vec::new();
        let mut before_matched:    Vec<(u32, usize)> = Vec::new();
        let mut after_matched:     Vec<(u32, usize)> = Vec::new();
        let mut selection_matched: Vec<(u32, usize)> = Vec::new();
        let mut marker_matched:    Vec<(u32, usize)> = Vec::new();

        for &rule_idx in candidates_buf.iter() {
            let rule = &stylesheet.rules[rule_idx];
            if !rule.media_condition.is_empty() && !evaluate_media(&rule.media_condition, vw, vh) {
                continue;
            }
            if !rule.container_condition.is_empty() { continue; }
            for sel in &rule.selectors {
                let has_hover   = sel.parts.iter().any(|p| matches!(p, SelectorPart::PseudoClass(n) if n == "hover"));
                let has_active  = sel.parts.iter().any(|p| matches!(p, SelectorPart::PseudoClass(n) if n == "active"));
                let has_visited = sel.parts.iter().any(|p| matches!(p, SelectorPart::PseudoClass(n) if n == "visited"));

                if (has_hover || has_active || has_visited) && rule.pseudo_element == PseudoElement::None {
                    let base_parts: Vec<SelectorPart> = sel.parts.iter()
                        .filter(|p| !matches!(p, SelectorPart::PseudoClass(n)
                            if matches!(n.as_str(), "hover" | "active" | "visited")))
                        .cloned()
                        .collect();
                    let base_sel = CssSelector::new(base_parts);
                    if base_sel.matches_with_ancestors_ctx_raw(
                        &item.tag, &item.attributes,
                        item.child_index, item.sibling_count,
                        &item.ancestors, &match_ctx,
                    ) {
                        if has_hover   { hover_matched.push((rule.specificity, rule_idx)); }
                        if has_active  { active_matched.push((rule.specificity, rule_idx)); }
                        if has_visited { visited_matched.push((rule.specificity, rule_idx)); }
                        if has_hover && !hover_chain.is_empty() {
                            if sel.matches_with_ancestors_ctx_raw(
                                &item.tag, &item.attributes,
                                item.child_index, item.sibling_count,
                                &item.ancestors, &match_ctx,
                            ) {
                                matched.push((rule.specificity, rule_idx));
                            }
                        }
                        break;
                    }
                    continue;
                }
                if sel.matches_with_ancestors_ctx_raw(
                    &item.tag, &item.attributes,
                    item.child_index, item.sibling_count,
                    &item.ancestors, &match_ctx,
                ) {
                    match rule.pseudo_element {
                        PseudoElement::Before     => before_matched.push((rule.specificity, rule_idx)),
                        PseudoElement::After      => after_matched.push((rule.specificity, rule_idx)),
                        PseudoElement::Selection  => selection_matched.push((rule.specificity, rule_idx)),
                        PseudoElement::Marker     => marker_matched.push((rule.specificity, rule_idx)),
                        PseudoElement::None       => matched.push((rule.specificity, rule_idx)),
                        PseudoElement::Ignored    => {}
                    }
                    break;
                }
            }
        }

        CascadeMatchResult {
            node_path: item.node_path.clone(),
            matched,
            hover_matched,
            active_matched,
            visited_matched,
            before_matched,
            after_matched,
            selection_matched,
            marker_matched,
        }
    }).collect()
}

/// Pass 3: Walk tree sequentially, apply matched rules to each node's style.
/// Uses the match results from the parallel pass instead of re-matching selectors.
fn apply_matched_results(
    root: &mut crate::types::HtmlBox,
    stylesheet: &Stylesheet,
    parent_style: Option<&ComputedStyle>,
    root_font_px: f32,
    ancestors: &mut Vec<AncestorInfo>,
    child_index: usize,
    sibling_count: usize,
    type_child_index: usize,
    type_sibling_count: usize,
    vw: f32,
    vh: f32,
    inherited_vars: &HashMap<String, String>,
    counters: &mut HashMap<String, Vec<i32>>,
    hover_chain: &std::collections::HashSet<u32>,
    results_map: &HashMap<Vec<usize>, &CascadeMatchResult>,
    current_path: &mut Vec<usize>,
) {
    if ancestors.len() >= MAX_CASCADE_DEPTH {
        if let Some(p) = parent_style {
            root.style.inherit_from(p);
        }
        return;
    }

    if root.tag == "#text" {
        if let Some(p) = parent_style {
            root.style.inherit_from(p);
        }
        return;
    }

    if root.tag == "::before" || root.tag == "::after" {
        if let Some(p) = parent_style {
            let saved_display = root.style.display;
            root.style.inherit_from(p);
            root.style.display = saved_display;
        }
        return;
    }

    // Look up pre-computed match results for this node.
    let result = results_map.get(current_path);

    // Start with default style and inherit from parent
    let mut style = ComputedStyle::default();
    if let Some(p) = parent_style {
        style.inherit_from(p);
    }

    // Apply presentational HTML attributes (specificity 0 — before author rules)
    let attrs = root.attributes.clone();
    for (attr, val) in &attrs {
        match attr.as_str() {
            "align" => match val.as_str() {
                "center" => apply_property(&mut style, "text-align", "center"),
                "right"  => apply_property(&mut style, "text-align", "right"),
                "left"   => apply_property(&mut style, "text-align", "left"),
                _ => {}
            },
            "valign"  => apply_property(&mut style, "vertical-align", val),
            "bgcolor" => apply_property(&mut style, "background-color", val),
            "color" | "text" => apply_property(&mut style, "color", val),
            "face"  => apply_property(&mut style, "font-family", val),
            "size"  => {
                let px: f32 = match val.trim() {
                    "1" => 10.0, "2" => 13.0, "3" => 16.0,
                    "4" => 18.0, "5" => 24.0, "6" => 32.0, "7" => 48.0,
                    v   => v.parse::<f32>().unwrap_or(16.0),
                };
                apply_property(&mut style, "font-size", &format!("{}px", px));
            }
            "width" => {
                if val.ends_with('%') {
                    apply_property(&mut style, "width", val);
                } else if val.parse::<f32>().is_ok() {
                    apply_property(&mut style, "width", &format!("{}px", val));
                }
            }
            "height" => {
                if val.ends_with('%') {
                    apply_property(&mut style, "height", val);
                } else if val.parse::<f32>().is_ok() {
                    apply_property(&mut style, "height", &format!("{}px", val));
                }
            }
            "border" if root.tag == "table" => {
                if let Ok(w) = val.parse::<f32>() {
                    if w > 0.0 {
                        apply_property(&mut style, "border", &format!("{}px solid", w));
                        apply_property(&mut style, "border-collapse", "collapse");
                    } else {
                        apply_property(&mut style, "border", "0px solid transparent");
                    }
                }
            }
            "cellspacing" => {
                if let Ok(n) = val.parse::<f32>() {
                    apply_property(&mut style, "border-spacing", &format!("{}px", n));
                } else if val.ends_with("px") {
                    apply_property(&mut style, "border-spacing", val);
                }
            }
            "cellpadding" => {
                apply_property(&mut style, "cellpadding", val);
            }
            "dir" => match val.to_ascii_lowercase().as_str() {
                "rtl" => apply_property(&mut style, "direction", "rtl"),
                _     => apply_property(&mut style, "direction", "ltr"),
            },
            _ => {}
        }
    }

    if matches!(root.tag.as_str(), "td" | "th") {
        let has_table_border = ancestors.iter().rev().any(|a| {
            a.tag == "table" && a.attributes.get("border")
                .and_then(|v| v.parse::<f32>().ok())
                .map_or(false, |n| n > 0.0)
        });
        if has_table_border {
            apply_property(&mut style, "border", "1px solid");
        }
    }

    // Apply matched rules from parallel pass (or empty if not found).
    let empty_result = CascadeMatchResult {
        node_path: Vec::new(),
        matched: Vec::new(), hover_matched: Vec::new(),
        active_matched: Vec::new(), visited_matched: Vec::new(),
        before_matched: Vec::new(), after_matched: Vec::new(),
        selection_matched: Vec::new(), marker_matched: Vec::new(),
    };
    let r = result.copied().unwrap_or(&empty_result);

    let mut matched = r.matched.clone();
    let mut hover_matched = r.hover_matched.clone();
    let mut active_matched = r.active_matched.clone();
    let mut visited_matched = r.visited_matched.clone();
    let mut before_matched = r.before_matched.clone();
    let mut after_matched = r.after_matched.clone();
    let mut selection_matched = r.selection_matched.clone();
    let mut marker_matched = r.marker_matched.clone();

    matched.sort_by_key(|(sp, _)| *sp);

    // Build variable scope
    let has_new_vars = matched.iter().any(|(_, ri)| {
        stylesheet.rules[*ri].declarations.keys().any(|p| p.starts_with("--"))
        || stylesheet.rules[*ri].important_declarations.keys().any(|p| p.starts_with("--"))
    });
    let inline_decls = root.attributes.get("style").cloned()
        .map(|s| parse_declarations_important(&s));
    let has_inline_vars = inline_decls.as_ref()
        .map(|(n, _)| n.keys().any(|p| p.starts_with("--")))
        .unwrap_or(false);

    let mut local_vars_owned = HashMap::new();
    let local_vars: &HashMap<String, String> = if has_new_vars || has_inline_vars {
        let mut vars = inherited_vars.clone();
        for &(_, ri) in &matched {
            for (prop, val) in &stylesheet.rules[ri].declarations {
                if prop.starts_with("--") { vars.insert(prop.clone(), val.clone()); }
            }
            for (prop, val) in &stylesheet.rules[ri].important_declarations {
                if prop.starts_with("--") { vars.insert(prop.clone(), val.clone()); }
            }
        }
        if let Some((ref n, _)) = inline_decls {
            for (prop, val) in n {
                if prop.starts_with("--") { vars.insert(prop.clone(), val.clone()); }
            }
        }
        pre_resolve_variables(&mut vars);
        local_vars_owned = vars;
        &local_vars_owned
    } else {
        inherited_vars
    };

    // Apply normal declarations
    let mut inherit_props: HashSet<String> = HashSet::new();
    for &(_, ri) in &matched {
        for (prop, val) in &stylesheet.rules[ri].declarations {
            if prop.starts_with("--") { continue; }
            let resolved = resolve_var_references(val, local_vars);
            if resolved.trim().is_empty() && val.contains("var(") { continue; }
            if resolved.trim() == "inherit" {
                inherit_props.insert(prop.to_string());
            } else {
                inherit_props.remove(prop.as_str());
                apply_property(&mut style, prop, &resolved);
            }
        }
    }
    if !inherit_props.is_empty() {
        if let Some(p) = parent_style {
            for prop in &inherit_props {
                copy_property_from_parent(&mut style, p, prop);
            }
        }
    }

    // Hover style
    if !hover_matched.is_empty() {
        hover_matched.sort_by_key(|(sp, _)| *sp);
        let mut hs = style.clone();
        for &(_, ri) in &hover_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_decls {
                let resolved = resolve_var_references(val, local_vars);
                if resolved.trim().is_empty() && val.contains("var(") { continue; }
                apply_property_by_id(&mut hs, id, &resolved);
            }
        }
        for &(_, ri) in &hover_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
                let resolved = resolve_var_references(val, local_vars);
                if resolved.trim().is_empty() && val.contains("var(") { continue; }
                apply_property_by_id(&mut hs, id, &resolved);
            }
        }
        hs.hover_style = None; hs.active_style = None; hs.visited_style = None;
        style.hover_style = Some(Box::new(hs));
    }
    // Active style
    if !active_matched.is_empty() {
        active_matched.sort_by_key(|(sp, _)| *sp);
        let mut as_ = style.clone();
        for &(_, ri) in &active_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_decls {
                let resolved = resolve_var_references(val, local_vars);
                if resolved.trim().is_empty() && val.contains("var(") { continue; }
                apply_property_by_id(&mut as_, id, &resolved);
            }
        }
        for &(_, ri) in &active_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
                let resolved = resolve_var_references(val, local_vars);
                if resolved.trim().is_empty() && val.contains("var(") { continue; }
                apply_property_by_id(&mut as_, id, &resolved);
            }
        }
        as_.hover_style = None; as_.active_style = None; as_.visited_style = None;
        style.active_style = Some(Box::new(as_));
    }
    // Visited style
    if !visited_matched.is_empty() {
        visited_matched.sort_by_key(|(sp, _)| *sp);
        let mut vs = style.clone();
        for &(_, ri) in &visited_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_decls {
                let resolved = resolve_var_references(val, local_vars);
                if resolved.trim().is_empty() && val.contains("var(") { continue; }
                apply_property_by_id(&mut vs, id, &resolved);
            }
        }
        for &(_, ri) in &visited_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
                let resolved = resolve_var_references(val, local_vars);
                if resolved.trim().is_empty() && val.contains("var(") { continue; }
                apply_property_by_id(&mut vs, id, &resolved);
            }
        }
        vs.hover_style = None; vs.active_style = None; vs.visited_style = None;
        style.visited_style = Some(Box::new(vs));
    }

    // Apply inline style
    let mut inline_hover_props: Vec<(String, String)> = Vec::new();
    let (_inline_normal, inline_important) = if let Some((n, i)) = inline_decls {
        for (prop, val) in &n {
            if prop.starts_with("--") { continue; }
            if let Some(real_prop) = prop.strip_prefix("hover-") {
                let resolved = resolve_var_references(val, local_vars);
                inline_hover_props.push((real_prop.to_string(), resolved));
                continue;
            }
            let resolved = resolve_var_references(val, local_vars);
            if resolved.trim() == "inherit" {
                if let Some(p) = parent_style { copy_property_from_parent(&mut style, p, prop); }
            } else {
                apply_property(&mut style, prop, &resolved);
            }
        }
        (n, i)
    } else {
        (HashMap::new(), HashMap::new())
    };
    if !inline_hover_props.is_empty() {
        let mut hs = if let Some(existing) = style.hover_style.take() {
            *existing
        } else {
            style.clone()
        };
        for (prop, val) in &inline_hover_props {
            apply_property(&mut hs, prop, val);
        }
        hs.hover_style = None; hs.active_style = None; hs.visited_style = None;
        style.hover_style = Some(Box::new(hs));
    }

    // !important stylesheet declarations
    for &(_, ri) in &matched {
        for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
            let resolved = resolve_var_references(val, local_vars);
            if resolved.trim().is_empty() && val.contains("var(") { continue; }
            apply_property_by_id(&mut style, id, &resolved);
        }
    }

    // !important inline declarations
    for (prop, val) in &inline_important {
        let resolved = resolve_var_references(val, local_vars);
        if resolved.trim() == "inherit" {
            if let Some(p) = parent_style { copy_property_from_parent(&mut style, p, prop); }
        } else {
            apply_property(&mut style, prop, &resolved);
        }
    }

    // Re-apply table layout HTML attributes
    if root.tag == "table" {
        if let Some(v) = root.attributes.get("cellspacing").cloned() {
            if let Ok(n) = v.parse::<f32>() {
                apply_property(&mut style, "border-spacing", &format!("{}px", n));
            }
        }
        if let Some(v) = root.attributes.get("cellpadding").cloned() {
            apply_property(&mut style, "cellpadding", &v);
        }
        if let Some(v) = root.attributes.get("border").cloned() {
            if let Ok(n) = v.parse::<f32>() {
                if n > 0.0 {
                    apply_property(&mut style, "border-collapse", "collapse");
                }
            }
        }
    }

    if let Some(href) = root.attributes.get("href") {
        style.href = href.clone();
    }

    let parent_font_px = parent_style.map(|p| p.font_size_px(root_font_px, root_font_px)).unwrap_or(root_font_px);
    let font_px = style.font_size_px(parent_font_px, root_font_px);
    style.font_size = CssLength::Px(font_px);

    let root_font_px = if root.tag.eq_ignore_ascii_case("html") {
        font_px
    } else {
        root_font_px
    };

    style.list_index = root.style.list_index;
    if matches!(style.display, Display::Inline) {
        let has_explicit_display = matched.iter().any(|&(_, ri)| {
            stylesheet.rules[ri].declarations.iter().any(|(k, _)| k == "display")
        });
        if !has_explicit_display {
            let should_be_block = matches!(root.tag.as_str(),
                "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                | "ul" | "ol" | "dl" | "dt" | "dd" | "pre" | "blockquote" | "hr"
                | "section" | "article" | "aside" | "nav" | "header" | "footer" | "main"
                | "address" | "figure" | "figcaption" | "details" | "center"
                | "form" | "fieldset" | "legend" | "hgroup" | "search");
            if should_be_block {
                style.display = Display::Block;
            }
        }
    }
    root.style = style.clone();

    if stylesheet.inspect_mode {
        root.matched_rules.clear();
        for &(sp, ri) in &matched {
            let rule = &stylesheet.rules[ri];
            root.matched_rules.push(crate::types::MatchedRule {
                selector: rule.original_selector.clone(),
                declarations: rule.declarations.iter()
                    .map(|(k, v)| (k.clone(), resolve_var_references(v, local_vars)))
                    .collect(),
                specificity: sp,
                source: if ri < 50 { "ua".to_string() }
                        else { rule.media_condition.clone() },
            });
        }
    }

    root.layout.layout_dirty = true;

    if root.tag == "form" {
        let in_table = ancestors.iter().any(|a|
            matches!(a.tag.as_str(), "table" | "thead" | "tbody" | "tfoot" | "tr"));
        if in_table {
            root.style.display = Display::Contents;
        }
    }

    // Build pseudo-element styles
    let build_pseudo_style = |matched: &mut Vec<(u32, usize)>,
                               base: &ComputedStyle,
                               vars: &HashMap<String, String>,
                               rules: &[CssRule]|
     -> Option<(String, Box<ComputedStyle>)> {
        if matched.is_empty() { return None; }
        matched.sort_by_key(|(sp, _)| *sp);
        let mut ps = base.clone();
        ps.before_style = None;
        ps.after_style  = None;
        ps.before_content = String::new();
        ps.after_content  = String::new();
        let mut content = String::new();
        for &(_, ri) in matched.iter() {
            for (prop, val) in &rules[ri].declarations {
                let resolved = resolve_var_references(val, vars);
                if prop == "content" {
                    content = resolve_content_value(&resolved);
                } else {
                    apply_property(&mut ps, prop, &resolved);
                }
            }
        }
        for &(_, ri) in matched.iter() {
            for (prop, val) in &rules[ri].important_declarations {
                let resolved = resolve_var_references(val, vars);
                if prop == "content" {
                    content = resolve_content_value(&resolved);
                } else {
                    apply_property(&mut ps, prop, &resolved);
                }
            }
        }
        Some((content, Box::new(ps)))
    };

    // CSS counters
    let mut counters_pushed: Vec<String> = Vec::new();
    for (name, val) in &root.style.counter_reset {
        counters.entry(name.clone()).or_insert_with(Vec::new).push(*val);
        counters_pushed.push(name.clone());
    }
    if root.tag == "ol" && root.style.counter_reset.is_empty() {
        counters.entry("list-item".to_string()).or_insert_with(Vec::new).push(0);
        counters_pushed.push("list-item".to_string());
    }
    for (name, val) in &root.style.counter_increment {
        if let Some(stack) = counters.get_mut(name) {
            if let Some(top) = stack.last_mut() {
                *top += val;
            }
        }
    }
    if root.tag == "li" && root.style.counter_increment.is_empty() {
        if let Some(stack) = counters.get_mut("list-item") {
            if let Some(top) = stack.last_mut() {
                *top += 1;
            }
        }
    }

    if let Some((txt, ps)) = build_pseudo_style(&mut before_matched, &root.style, local_vars, &stylesheet.rules) {
        for (name, val) in &ps.counter_reset {
            counters.entry(name.clone()).or_insert_with(Vec::new).push(*val);
            counters_pushed.push(name.clone());
        }
        for (name, val) in &ps.counter_increment {
            if let Some(stack) = counters.get_mut(name) {
                if let Some(top) = stack.last_mut() {
                    *top += val;
                }
            }
        }
        root.style.before_content = resolve_counters_in_content(&txt, counters);
        root.style.before_style   = Some(ps);
    }
    if let Some((txt, ps)) = build_pseudo_style(&mut after_matched, &root.style, local_vars, &stylesheet.rules) {
        root.style.after_content = resolve_counters_in_content(&txt, counters);
        root.style.after_style   = Some(ps);
    }
    if let Some((_, ps)) = build_pseudo_style(&mut selection_matched, &root.style, local_vars, &stylesheet.rules) {
        root.style.selection_style = Some(ps);
    }
    if let Some((_, ps)) = build_pseudo_style(&mut marker_matched, &root.style, local_vars, &stylesheet.rules) {
        root.style.marker_style = Some(ps);
    }

    // Grid/flex ::before/::after child box creation
    let is_grid_or_flex = matches!(root.style.display,
        Display::Grid | Display::InlineGrid | Display::Flex | Display::InlineFlex);
    let before_is_positioned = root.style.before_style.as_ref().map_or(false, |ps|
        matches!(ps.position, Position::Absolute | Position::Fixed));
    if (is_grid_or_flex && !root.style.before_content.is_empty())
        || (before_is_positioned && root.style.before_style.is_some())
    {
        let existing = root.children.iter().position(|c| c.tag == "::before");
        let mut pseudo_box = {
            let mut b = crate::types::HtmlBox::new("::before");
            b.text = root.style.before_content.clone();
            b
        };
        pseudo_box.tag = "::before".to_string();
        if let Some(ref ps) = root.style.before_style {
            pseudo_box.style = *ps.clone();
        }
        if is_grid_or_flex && !pseudo_box.style.is_positioned()
            && matches!(pseudo_box.style.display, Display::Inline) {
            pseudo_box.style.display = Display::Block;
        }
        if let Some(idx) = existing {
            root.children[idx] = pseudo_box;
        } else {
            root.children.insert(0, pseudo_box);
        }
        root.style.before_content = String::new();
    } else {
        if let Some(idx) = root.children.iter().position(|c| c.tag == "::before") {
            root.children.remove(idx);
        }
    }
    let after_is_positioned = root.style.after_style.as_ref().map_or(false, |ps|
        matches!(ps.position, Position::Absolute | Position::Fixed));
    if (is_grid_or_flex && !root.style.after_content.is_empty())
        || (after_is_positioned && root.style.after_style.is_some())
    {
        let existing = root.children.iter().position(|c| c.tag == "::after");
        let mut pseudo_box = {
            let mut b = crate::types::HtmlBox::new("::after");
            b.text = root.style.after_content.clone();
            b
        };
        pseudo_box.tag = "::after".to_string();
        if let Some(ref ps) = root.style.after_style {
            pseudo_box.style = *ps.clone();
        }
        if is_grid_or_flex && !pseudo_box.style.is_positioned()
            && matches!(pseudo_box.style.display, Display::Inline) {
            pseudo_box.style.display = Display::Block;
        }
        if let Some(idx) = existing {
            root.children[idx] = pseudo_box;
        } else {
            root.children.push(pseudo_box);
        }
        root.style.after_content = String::new();
    } else {
        if let Some(idx) = root.children.iter().position(|c| c.tag == "::after") {
            root.children.remove(idx);
        }
    }

    // Push ancestor for children
    ancestors.push(AncestorInfo {
        tag:                root.tag.clone(),
        attributes:         root.attributes.clone(),
        child_index,
        sibling_count,
        type_child_index,
        type_sibling_count,
        node_id:            root.node_id,
    });

    // Recurse into children
    // Shadow DOM: cascade shadow children with the shadow's scoped stylesheet.
    // For shadow DOM, fall back to the sequential cascade since shadow stylesheets
    // were not included in the parallel pass.
    if root.shadow_root.is_some() {
        let mut sr = root.shadow_root.take().unwrap();
        sr.stylesheet.rebuild_index();
        // Shadow children use their own stylesheet — fall back to sequential cascade.
        let mut shadow_candidates_buf: Vec<usize> = Vec::new();
        cascade_children_sequential(
            &mut sr.children, &sr.stylesheet, &style, root_font_px,
            ancestors, vw, vh, 0, false,
            local_vars, &mut shadow_candidates_buf, counters, hover_chain,
        );
        root.shadow_root = Some(sr);
        // Light DOM children use document stylesheet — apply from parallel results.
        cascade_children_parallel(
            &mut root.children, stylesheet, &style, root_font_px,
            ancestors, vw, vh, inherited_vars, counters, hover_chain,
            results_map, current_path,
        );
    } else {
        cascade_children_parallel(
            &mut root.children, stylesheet, &style, root_font_px,
            ancestors, vw, vh, local_vars, counters, hover_chain,
            results_map, current_path,
        );
    }

    ancestors.pop();

    for name in counters_pushed.iter().rev() {
        if let Some(stack) = counters.get_mut(name) {
            stack.pop();
            if stack.is_empty() { counters.remove(name); }
        }
    }
}

/// Helper: cascade children using parallel match results.
fn cascade_children_parallel(
    children: &mut [crate::types::HtmlBox],
    stylesheet: &Stylesheet,
    parent_style: &ComputedStyle,
    root_font_px: f32,
    ancestors: &mut Vec<AncestorInfo>,
    vw: f32, vh: f32,
    inherited_vars: &HashMap<String, String>,
    counters: &mut HashMap<String, Vec<i32>>,
    hover_chain: &std::collections::HashSet<u32>,
    results_map: &HashMap<Vec<usize>, &CascadeMatchResult>,
    current_path: &mut Vec<usize>,
) {
    let n_children = children.len();
    if n_children == 0 { return; }
    let child_tags: Vec<String> = children.iter().map(|c| c.tag.to_ascii_lowercase()).collect();
    let mut type_running: HashMap<&str, usize> = HashMap::new();
    let type_counts: Vec<usize> = child_tags.iter().map(|tag| {
        let slot = type_running.entry(tag.as_str()).or_insert(0);
        let idx = *slot; *slot += 1; idx
    }).collect();
    let type_totals: Vec<usize> = child_tags.iter().map(|tag| {
        *type_running.get(tag.as_str()).unwrap_or(&0)
    }).collect();
    let n_elem_children = children.iter().filter(|c| c.tag != "#text").count();
    let mut elem_pos = 0usize;
    let elem_indices: Vec<usize> = children.iter().map(|c| {
        if c.tag == "#text" { 0 } else { let p = elem_pos; elem_pos += 1; p }
    }).collect();
    for (i, child) in children.iter_mut().enumerate() {
        let (ci, ns) = if child.tag == "#text" {
            (i, n_children)
        } else {
            (elem_indices[i], n_elem_children)
        };
        current_path.push(i);
        apply_matched_results(
            child, stylesheet, Some(parent_style), root_font_px,
            ancestors, ci, ns,
            type_counts[i], type_totals[i],
            vw, vh, inherited_vars, counters, hover_chain,
            results_map, current_path,
        );
        current_path.pop();
    }
}

/// Helper: cascade children sequentially (used for shadow DOM subtrees).
fn cascade_children_sequential(
    children: &mut [crate::types::HtmlBox],
    stylesheet: &Stylesheet,
    parent_style: &ComputedStyle,
    root_font_px: f32,
    ancestors: &mut Vec<AncestorInfo>,
    vw: f32, vh: f32,
    focused_box: u32,
    keyboard_focus: bool,
    inherited_vars: &HashMap<String, String>,
    candidates_buf: &mut Vec<usize>,
    counters: &mut HashMap<String, Vec<i32>>,
    hover_chain: &std::collections::HashSet<u32>,
) {
    let n_children = children.len();
    if n_children == 0 { return; }
    let child_tags: Vec<String> = children.iter().map(|c| c.tag.to_ascii_lowercase()).collect();
    let mut type_running: HashMap<&str, usize> = HashMap::new();
    let type_counts: Vec<usize> = child_tags.iter().map(|tag| {
        let slot = type_running.entry(tag.as_str()).or_insert(0);
        let idx  = *slot; *slot += 1; idx
    }).collect();
    let type_totals: Vec<usize> = child_tags.iter().map(|tag| {
        *type_running.get(tag.as_str()).unwrap_or(&0)
    }).collect();
    let n_elem_children = children.iter().filter(|c| c.tag != "#text").count();
    let mut elem_pos = 0usize;
    let elem_indices: Vec<usize> = children.iter().map(|c| {
        if c.tag == "#text" { 0 } else { let p = elem_pos; elem_pos += 1; p }
    }).collect();
    for (i, child) in children.iter_mut().enumerate() {
        let (ci, ns) = if child.tag == "#text" {
            (i, n_children)
        } else {
            (elem_indices[i], n_elem_children)
        };
        apply_cascade_inner(
            child, stylesheet, Some(parent_style), root_font_px,
            ancestors, ci, ns,
            type_counts[i], type_totals[i],
            vw, vh, focused_box, keyboard_focus,
            inherited_vars, candidates_buf, counters,
            hover_chain,
        );
    }
}

/// Parallel cascade: 3-pass approach for large stylesheets.
/// 1. Flatten DOM into work list with ancestor snapshots (sequential)
/// 2. Run selector matching in parallel via Rayon (parallel)
/// 3. Apply matched rules to styles (sequential — inherits from parent, builds state styles)
pub fn apply_cascade_parallel(
    root: &mut crate::types::HtmlBox,
    stylesheet: &Stylesheet,
    parent_style: Option<&ComputedStyle>,
    root_font_px: f32,
    vw: f32,
    vh: f32,
    focused_box: u32,
    keyboard_focus: bool,
    hover_chain: &std::collections::HashSet<u32>,
) {
    // Pass 1: Flatten the tree.
    let mut work_items: Vec<CascadeWorkItem> = Vec::new();
    let mut ancestors: Vec<AncestorInfo> = Vec::new();
    let mut path: Vec<usize> = Vec::new();
    flatten_tree_for_cascade(root, &mut ancestors, &mut path, 0, 1, 0, 1, &mut work_items);

    // Pass 2: Parallel selector matching.
    let match_results = parallel_selector_match(
        &work_items, stylesheet, vw, vh, focused_box, keyboard_focus, hover_chain,
    );

    // Build lookup map: node_path -> &CascadeMatchResult
    let results_map: HashMap<Vec<usize>, &CascadeMatchResult> = match_results.iter()
        .map(|r| (r.node_path.clone(), r))
        .collect();

    // Pass 3: Sequential style application.
    let mut ancestors: Vec<AncestorInfo> = Vec::new();
    let mut counters: HashMap<String, Vec<i32>> = HashMap::new();
    let mut current_path: Vec<usize> = Vec::new();
    apply_matched_results(
        root, stylesheet, parent_style, root_font_px,
        &mut ancestors, 0, 1, 0, 1,
        vw, vh, &stylesheet.variables, &mut counters, hover_chain,
        &results_map, &mut current_path,
    );
}

// ─── User-Agent Stylesheet ───────────────────────────────────────────────────

pub fn ua_stylesheet() -> Stylesheet {
    let mut ss = Stylesheet::default();
    ss.parse_and_add(UA_CSS);
    ss
}

const UA_CSS: &str = r##"
head, link, meta, script, style, title { display: none; }
area, base, basefont, datalist, noembed, noframes, param, rp, source, template { display: none; }
picture { display: contents; }
[hidden] { display: none; }
html { display: block; }
body { display: block; margin: 8px; }
article, aside, nav, section { display: block; }
h1 { display: block; font-size: 2em; font-weight: bold; margin-top: 0.67em; margin-bottom: 0.67em; break-after: avoid; break-inside: avoid; }
article h1, aside h1, nav h1, section h1 { font-size: 1.5em; margin-top: 0.83em; margin-bottom: 0.83em; }
article article h1, article aside h1, article nav h1, article section h1, aside article h1, aside aside h1, aside nav h1, aside section h1, nav article h1, nav aside h1, nav nav h1, nav section h1, section article h1, section aside h1, section nav h1, section section h1 { font-size: 1.17em; margin-top: 1em; margin-bottom: 1em; }
h2 { display: block; font-size: 1.5em; font-weight: bold; margin-top: 0.83em; margin-bottom: 0.83em; break-after: avoid; break-inside: avoid; }
h3 { display: block; font-size: 1.17em; font-weight: bold; margin-top: 1em; margin-bottom: 1em; break-after: avoid; break-inside: avoid; }
h4 { display: block; font-size: 1em; font-weight: bold; margin-top: 1.33em; margin-bottom: 1.33em; break-after: avoid; break-inside: avoid; }
h5 { display: block; font-size: 0.83em; font-weight: bold; margin-top: 1.67em; margin-bottom: 1.67em; break-after: avoid; break-inside: avoid; }
h6 { display: block; font-size: 0.67em; font-weight: bold; margin-top: 2.33em; margin-bottom: 2.33em; break-after: avoid; break-inside: avoid; }
hgroup { display: block; }
div, header, footer, main, search { display: block; }
form { display: block; }
p  { display: block; margin-top: 1em; margin-bottom: 1em; }
address { display: block; font-style: italic; }
blockquote { display: block; margin-top: 1em; margin-bottom: 1em; margin-left: 40px; margin-right: 40px; }
center { display: block; text-align: center; }
figure { display: block; margin-top: 1em; margin-bottom: 1em; margin-left: 40px; margin-right: 40px; }
figcaption { display: block; }
details { display: block; }
summary { display: list-item; list-style-type: disclosure-closed; }
pre, listing, plaintext, xmp { display: block; font-family: monospace; white-space: pre; margin-top: 1em; margin-bottom: 1em; }
hr  { display: block; margin-top: 0.5em; margin-bottom: 0.5em; margin-left: auto; margin-right: auto; height: 0; border-top-width: 1px; border-top-style: solid; border-top-color: silver; overflow: hidden; }
dl, ol, ul, menu, dir { display: block; margin-top: 1em; margin-bottom: 1em; }
ol, ul, menu { padding-left: 40px; }
menu { list-style-type: disc; }
dir  { list-style-type: disc; padding-left: 40px; }
dd, dt { display: block; }
dd { margin-left: 40px; }
li { display: list-item; }
ol { list-style-type: decimal; }
ul { list-style-type: disc; }
ul ul, ul ol, ul menu, ol ul, ol ol, ol menu, menu ul, menu ol, menu menu,
dir ul, dir ol, dir menu, dir dir { margin-top: 0; margin-bottom: 0; }
ul ul, ol ul, menu ul { list-style-type: circle; }
ul ul ul, ul ol ul, ol ul ul, ol ol ul, menu ul ul { list-style-type: square; }
cite, dfn, em, i, var { font-style: italic; }
b, strong { font-weight: bold; }
code, kbd, samp, tt { font-family: monospace; }
small { font-size: 0.83em; }
big  { font-size: 1.17em; }
sub  { vertical-align: sub; font-size: 0.83em; line-height: normal; }
sup  { vertical-align: super; font-size: 0.83em; line-height: normal; }
mark { background-color: yellow; color: black; }
a { color: #0000ee; text-decoration: underline; cursor: pointer; }
:visited { color: #551a8b; }
u, ins { text-decoration: underline; }
s, strike, del { text-decoration: line-through; }
abbr[title], acronym[title] { text-decoration: underline dotted; }
q::before { content: open-quote; }
q::after  { content: close-quote; }
nobr { white-space: nowrap; }
wbr  { display: inline; }
br { display: inline; }
img, svg { display: inline-block; break-inside: avoid; }
canvas, video { display: inline-block; }
audio { display: inline; }
iframe { display: inline-block; border: 2px inset; }
output { display: inline; }
table { display: table; border-collapse: separate; border-spacing: 2px; box-sizing: border-box; }
caption { display: table-caption; text-align: center; }
colgroup { display: table-column-group; }
col { display: table-column; }
thead { display: table-header-group; }
tbody { display: table-row-group; }
tfoot { display: table-footer-group; }
tr    { display: table-row; }
td, th { display: table-cell; padding: 1px; }
th { font-weight: bold; text-align: center; }
thead, tbody, tfoot, tr { vertical-align: middle; }
button, input[type=submit], input[type=button], input[type=reset] {
  display: inline-flex; align-items: center; justify-content: center;
  padding: 1px 6px; cursor: default; background-color: #e8e8e8; border: 1px solid #767676;
  white-space: nowrap; border-radius: 3px;
}
button:hover, input[type=submit]:hover, input[type=button]:hover, input[type=reset]:hover {
  background-color: #e0e0e0; border-color: #666;
}
input:focus, select:focus, textarea:focus {
  border-color: #4285f4; outline: none;
}
input:disabled, select:disabled, textarea:disabled, button:disabled {
  opacity: 0.6; cursor: default;
}
input[type=hidden] { display: none; }
input[type=radio], input[type=checkbox] { display: inline-block; width: 16px; height: 16px; vertical-align: middle; margin: 0 6px 0 2px; border: none; padding: 0; background: transparent; flex-shrink: 0; }
label { display: inline-block; }
input { display: inline-block; width: 200px; height: 2.2em; padding: 0 6px; border: 1px solid #ababab; border-radius: 3px; box-sizing: border-box; vertical-align: middle; background-color: #ffffff; color: #000000; }
input[type=submit], input[type=button], input[type=reset] { width: auto; height: auto; border: 1px solid #767676; padding: 3px 8px; background-color: #e8e8e8; }
select { display: inline-block; width: 200px; height: 2.2em; padding: 0 6px; border: 1px solid #ababab; border-radius: 3px; box-sizing: border-box; vertical-align: middle; background-color: #ffffff; color: #000000; }
option, optgroup { display: none; }
textarea { display: inline-block; white-space: pre-wrap; width: 200px; height: 3em; padding: 2px; border: 1px solid #767676; box-sizing: border-box; }
input[type=range] { width: 160px; height: 1.2em; border: none; padding: 0; }
input[type=color] { width: 44px; height: 23px; padding: 1px 2px; border: 1px solid #767676; box-sizing: border-box; }
input[type=file] { width: 240px; height: 1.6em; border: none; padding: 0; }
input[type=date], input[type=time], input[type=datetime-local], input[type=month], input[type=week] {
  width: 160px; height: 1.4em; padding: 1px 2px; border: 1px solid #767676; box-sizing: border-box;
}
progress { display: inline-block; width: 160px; height: 16px; vertical-align: middle; }
meter { display: inline-block; width: 80px; height: 16px; vertical-align: middle; }
output { display: inline; }
fieldset { display: block; margin-left: 2px; margin-right: 2px; padding-top: 0.35em; padding-bottom: 0.625em; padding-left: 0.75em; padding-right: 0.75em; border: 2px groove #ccc; }
legend { padding-left: 2px; padding-right: 2px; }
bdo { unicode-bidi: bidi-override; }
bdi { unicode-bidi: isolate; }
ruby { display: ruby; }
rt   { display: ruby-text; font-size: 0.5em; }
:focus-visible {
  outline-width: 2px;
  outline-style: solid;
  outline-color: #005fcc;
  outline-offset: 2px;
}
"##;
