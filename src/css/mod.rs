use std::collections::HashMap;
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
}

#[derive(Clone, Debug, PartialEq)]
pub enum AttrOp { Exists, Eq, Contains, StartsWith, EndsWith, Includes, DashMatch }

#[derive(Clone, Debug, PartialEq)]
pub enum Combinator { Descendant, Child, AdjacentSibling, GeneralSibling }

#[derive(Clone, Debug)]
pub struct CssSelector {
    pub parts: Vec<SelectorPart>,
}

/// Info about one ancestor box, threaded through the cascade for selector matching.
#[derive(Clone, Debug)]
pub struct AncestorInfo {
    pub tag:           String,
    pub attributes:    HashMap<String, String>,
    pub child_index:   usize,   // 0-based position among parent's children
    pub sibling_count: usize,   // total children of parent
}

impl CssSelector {
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
                _ => {}
            }
        }
        ids * 100 + classes * 10 + elements
    }

    /// Match against `b` without ancestor context (for tests / simple selectors).
    pub fn matches_box(&self, b: &HtmlBox) -> bool {
        matches_selector_with_ancestors(&self.parts, &b.tag, &b.attributes, 0, 1, &[])
    }

    /// Match against `b` with full ancestor chain for combinator resolution.
    pub fn matches_with_ancestors(
        &self,
        b: &HtmlBox,
        child_index: usize,
        sibling_count: usize,
        ancestors: &[AncestorInfo],
    ) -> bool {
        matches_selector_with_ancestors(&self.parts, &b.tag, &b.attributes, child_index, sibling_count, ancestors)
    }
}

/// Recursively match a selector (parts slice) against a subject element + its ancestor chain.
/// Works right-to-left: the last segment matches the subject, preceding segments
/// must match ancestors according to the combinator between them.
fn matches_selector_with_ancestors(
    parts: &[SelectorPart],
    tag: &str,
    attrs: &HashMap<String, String>,
    child_index: usize,
    sibling_count: usize,
    ancestors: &[AncestorInfo],
) -> bool {
    if parts.is_empty() { return true; }

    // Find the rightmost combinator in `parts`
    let last_comb_pos = parts.iter().rposition(|p| matches!(p, SelectorPart::Combinator(_)));

    match last_comb_pos {
        None => {
            // No combinator — all parts must match the subject
            parts.iter().all(|p| matches_part_with_context(p, tag, attrs, child_index, sibling_count))
        }
        Some(pos) => {
            let combinator = match &parts[pos] {
                SelectorPart::Combinator(c) => c.clone(),
                _ => unreachable!(),
            };
            let left_parts  = &parts[..pos];
            let right_parts = &parts[pos + 1..];

            // Right parts must all match the subject
            if !right_parts.iter().all(|p| matches_part_with_context(p, tag, attrs, child_index, sibling_count)) {
                return false;
            }

            match combinator {
                Combinator::Descendant => {
                    // Left parts must match any ancestor
                    for (i, anc) in ancestors.iter().enumerate() {
                        if matches_selector_with_ancestors(
                            left_parts,
                            &anc.tag, &anc.attributes,
                            anc.child_index, anc.sibling_count,
                            &ancestors[..i],
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
                        matches_selector_with_ancestors(
                            left_parts,
                            &parent.tag, &parent.attributes,
                            parent.child_index, parent.sibling_count,
                            parent_ancestors,
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
        SelectorPart::PseudoClass(pc) => {
            let pc = pc.as_str();
            match pc {
                "first-child"  => child_index == 0,
                "last-child"   => child_index + 1 == sibling_count,
                "only-child"   => sibling_count == 1,
                "first-of-type" | "last-of-type" => false, // need type-tracking, skip
                "root"         => tag.eq_ignore_ascii_case("html"),
                "empty"        => false, // can't tell from style alone
                "link" | "visited" | "active" | "focus" | "checked" | "disabled" | "enabled"
                | "placeholder-shown" | "required" | "optional" | "valid" | "invalid" => false,
                "not(_)" => false, // would need recursive parsing, skip
                _ => {
                    // nth-child(expr) / nth-of-type(expr)
                    if let Some(inner) = pc.strip_prefix("nth-child(").and_then(|s| s.strip_suffix(')')) {
                        return nth_matches(inner, child_index + 1); // CSS is 1-based
                    }
                    if let Some(inner) = pc.strip_prefix("nth-last-child(").and_then(|s| s.strip_suffix(')')) {
                        let from_end = sibling_count - child_index; // 1-based from end
                        return nth_matches(inner, from_end);
                    }
                    false
                }
            }
        }
        SelectorPart::PseudoElement(_) => true,
        SelectorPart::Combinator(_)    => true,
    }
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
pub enum PseudoElement { None, Before, After }

impl Default for PseudoElement {
    fn default() -> Self { Self::None }
}

#[derive(Clone, Debug)]
pub struct CssRule {
    pub selectors:        Vec<CssSelector>,
    pub declarations:     HashMap<String, String>,
    pub specificity:      u32,     // max of all selectors
    pub media_condition:  String,  // non-empty if inside @media
    pub original_selector: String, // verbatim selector text for roundtrip
    pub is_hover:         bool,
    pub pseudo_element:   PseudoElement,
}

impl Default for CssRule {
    fn default() -> Self {
        Self {
            selectors:         Vec::new(),
            declarations:      HashMap::new(),
            specificity:       0,
            media_condition:   String::new(),
            original_selector: String::new(),
            is_hover:          false,
            pseudo_element:    PseudoElement::None,
        }
    }
}

// ─── Stylesheet ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct Stylesheet {
    pub rules:     Vec<CssRule>,
    pub variables: HashMap<String, String>,  // CSS custom properties from :root
}

impl Stylesheet {
    pub fn add_rule(&mut self, rule: CssRule) {
        self.rules.push(rule);
    }

    /// Parse a CSS string and append its rules. Also extracts CSS variables from `:root`.
    pub fn parse_and_add(&mut self, css: &str) {
        // Extract :root CSS variables first
        extract_root_variables(css, &mut self.variables);
        if let Some(rules) = parse_stylesheet(css) {
            for r in rules {
                self.rules.push(r);
            }
        }
    }
}

/// Extract CSS custom properties (--name: value) from `:root { }` blocks.
fn extract_root_variables(css: &str, vars: &mut HashMap<String, String>) {
    let cleaned = strip_css_comments(css);
    let mut s = cleaned.as_str();
    loop {
        s = s.trim_start();
        if s.is_empty() { break; }
        // Find ":root"
        let root_pos = match s.find(":root") {
            Some(p) => p,
            None    => break,
        };
        s = &s[root_pos + 5..];
        s = s.trim_start();
        if !s.starts_with('{') { continue; }
        let (block, rest) = consume_block(s);
        s = rest;
        // Parse declarations for --variables
        for decl in block.split(';') {
            let decl = decl.trim();
            if let Some(colon) = decl.find(':') {
                let prop = decl[..colon].trim();
                if prop.starts_with("--") {
                    let val = decl[colon+1..].trim().to_string();
                    vars.insert(prop.to_string(), val);
                }
            }
        }
    }
}

// ─── CSS Parser ──────────────────────────────────────────────────────────────

/// Parse a full stylesheet text into rules.
/// `parent_media` is non-empty when called recursively from inside an @media block.
pub fn parse_stylesheet(css: &str) -> Option<Vec<CssRule>> {
    parse_stylesheet_inner(css, "")
}

fn parse_stylesheet_inner(css: &str, parent_media: &str) -> Option<Vec<CssRule>> {
    let mut rules = Vec::new();
    let cleaned = strip_css_comments(css);
    let mut s = cleaned.as_str().trim();

    while !s.is_empty() {
        s = s.trim_start();
        if s.is_empty() { break; }

        // @rules
        if s.starts_with('@') {
            let at_lower = s.to_ascii_lowercase();

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
                // Just recursively parse as-is (without tracking container condition for now)
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

        let declarations = parse_declarations(decl_block);
        if declarations.is_empty() { continue; }

        // Split comma-separated selectors
        for sel_str in selector_text.split(',') {
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

/// Detect and strip `::before` / `::after` (and CSS2 `:before`/`:after`) from
/// a selector string.  Returns (cleaned_selector, PseudoElement).
fn strip_pseudo_element(sel: &str) -> (String, PseudoElement) {
    // ::before / ::after
    if let Some(pos) = sel.find("::") {
        let pe_str = sel[pos+2..].to_ascii_lowercase();
        if pe_str.starts_with("before") {
            let clean = format!("{}{}", &sel[..pos], &sel[pos+2+6..]).trim().to_string();
            let clean = if clean.is_empty() { "*".to_string() } else { clean };
            return (clean, PseudoElement::Before);
        }
        if pe_str.starts_with("after") {
            let clean = format!("{}{}", &sel[..pos], &sel[pos+2+5..]).trim().to_string();
            let clean = if clean.is_empty() { "*".to_string() } else { clean };
            return (clean, PseudoElement::After);
        }
    }
    // CSS2 single-colon :before / :after (not preceded by another colon)
    let sel_lower = sel.to_ascii_lowercase();
    for (kw, pe) in &[(":before", PseudoElement::Before), (":after", PseudoElement::After)] {
        if let Some(pos) = sel_lower.find(kw) {
            // make sure it's not ::before
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
            let prop  = decl[..colon].trim().to_ascii_lowercase();
            let value = decl[colon + 1..]
                .trim()
                .trim_end_matches("!important")
                .trim()
                .to_string();
            if !prop.is_empty() && !value.is_empty() {
                map.insert(prop, value);
            }
        }
    }
    map
}

/// Parse a single CSS selector string into a CssSelector.
pub fn parse_selector(s: &str) -> CssSelector {
    let mut parts = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '\n' => {
                chars.next();
                // Check for combinators
                let next_non_ws = chars.peek().copied();
                match next_non_ws {
                    Some('>') => { chars.next(); parts.push(SelectorPart::Combinator(Combinator::Child)); }
                    Some('+') => { chars.next(); parts.push(SelectorPart::Combinator(Combinator::AdjacentSibling)); }
                    Some('~') => { chars.next(); parts.push(SelectorPart::Combinator(Combinator::GeneralSibling)); }
                    _         => { parts.push(SelectorPart::Combinator(Combinator::Descendant)); }
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
                // consume optional (...) — keep args for nth-child etc.
                let full_name = if chars.peek() == Some(&'(') {
                    chars.next();
                    let args: String = chars.by_ref().take_while(|&c| c != ')').collect();
                    format!("{}({})", name, args)
                } else {
                    name
                };
                if is_elem {
                    parts.push(SelectorPart::PseudoElement(full_name));
                } else {
                    parts.push(SelectorPart::PseudoClass(full_name));
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

    CssSelector { parts }
}

fn read_ident(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut s = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            s.push(c);
            chars.next();
        } else {
            break;
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
    if (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
    {
        s[1..s.len()-1].to_string()
    } else {
        s.to_string()
    }
}

// ─── CSS Property Application ─────────────────────────────────────────────────

/// Apply a single CSS property/value pair to a ComputedStyle.
pub fn apply_property(style: &mut ComputedStyle, prop: &str, value: &str) {
    let v = value.trim();
    match prop {
        "display" => {
            style.display = match v {
                "none"               => Display::None,
                "block"              => Display::Block,
                "inline"             => Display::Inline,
                "inline-block"       => Display::InlineBlock,
                "flex"               => Display::Flex,
                "inline-flex"        => Display::InlineFlex,
                "grid"               => Display::Grid,
                "inline-grid"        => Display::InlineGrid,
                "table"              => Display::Table,
                "table-row"          => Display::TableRow,
                "table-cell"         => Display::TableCell,
                "table-caption"      => Display::TableCaption,
                "table-column"       => Display::TableColumn,
                "table-column-group" => Display::TableColumnGroup,
                "table-header-group" => Display::TableHeaderGroup,
                "table-footer-group" => Display::TableFooterGroup,
                "table-row-group"    => Display::TableRowGroup,
                "list-item"          => Display::ListItem,
                "flow-root"          => Display::FlowRoot,
                "contents"           => Display::Contents,
                "ruby"               => Display::Ruby,
                "ruby-text"          => Display::RubyText,
                _                    => Display::Inline,
            };
        }
        "position" => {
            style.position = match v {
                "static"   => Position::Static,
                "relative" => Position::Relative,
                "absolute" => Position::Absolute,
                "fixed"    => Position::Fixed,
                "sticky"   => Position::Sticky,
                _          => Position::Static,
            };
        }
        "float" => {
            style.float = match v {
                "left"  => Float::Left,
                "right" => Float::Right,
                _       => Float::None,
            };
        }
        "clear" => {
            style.clear = match v {
                "left"  => Clear::Left,
                "right" => Clear::Right,
                "both"  => Clear::Both,
                _       => Clear::None,
            };
        }
        "z-index"   => { style.z_index = v.parse().unwrap_or(0); }
        "overflow"  => {
            let ov = parse_overflow(v);
            style.overflow_x = ov;
            style.overflow_y = ov;
        }
        "overflow-x" => { style.overflow_x = parse_overflow(v); }
        "overflow-y" => { style.overflow_y = parse_overflow(v); }

        "width"      => { style.width      = parse_length(v); }
        "height"     => { style.height     = parse_length(v); }
        "min-width"  => { style.min_width  = parse_length(v); }
        "max-width"  => { style.max_width  = parse_length_or_none(v); }
        "min-height" => { style.min_height = parse_length(v); }
        "max-height" => { style.max_height = parse_length_or_none(v); }

        "margin"        => apply_shorthand_4(v, &mut style.margin_top, &mut style.margin_right, &mut style.margin_bottom, &mut style.margin_left, parse_length),
        "margin-top"    => { style.margin_top    = parse_length(v); }
        "margin-right"  => { style.margin_right  = parse_length(v); }
        "margin-bottom" => { style.margin_bottom = parse_length(v); }
        "margin-left"   => { style.margin_left   = parse_length(v); }

        "padding"        => apply_shorthand_4(v, &mut style.padding_top, &mut style.padding_right, &mut style.padding_bottom, &mut style.padding_left, parse_length),
        "padding-top"    => { style.padding_top    = parse_length(v); }
        "padding-right"  => { style.padding_right  = parse_length(v); }
        "padding-bottom" => { style.padding_bottom = parse_length(v); }
        "padding-left"   => { style.padding_left   = parse_length(v); }

        "border"        => apply_border_shorthand(style, v),
        "border-width"  => apply_shorthand_4(v, &mut style.border_top_width, &mut style.border_right_width, &mut style.border_bottom_width, &mut style.border_left_width, parse_length),
        "border-top-width"    => { style.border_top_width    = parse_length(v); }
        "border-right-width"  => { style.border_right_width  = parse_length(v); }
        "border-bottom-width" => { style.border_bottom_width = parse_length(v); }
        "border-left-width"   => { style.border_left_width   = parse_length(v); }

        "border-style"        => {
            let bs = parse_border_style(v);
            style.border_top_style    = bs;
            style.border_right_style  = bs;
            style.border_bottom_style = bs;
            style.border_left_style   = bs;
        }
        "border-top-style"    => { style.border_top_style    = parse_border_style(v); }
        "border-right-style"  => { style.border_right_style  = parse_border_style(v); }
        "border-bottom-style" => { style.border_bottom_style = parse_border_style(v); }
        "border-left-style"   => { style.border_left_style   = parse_border_style(v); }

        "border-color"        => {
            let bc = parse_color(v).unwrap_or(Color::BLACK);
            style.border_top_color    = bc;
            style.border_right_color  = bc;
            style.border_bottom_color = bc;
            style.border_left_color   = bc;
        }
        "border-top-color"    => { if let Some(c) = parse_color(v) { style.border_top_color    = c; } }
        "border-right-color"  => { if let Some(c) = parse_color(v) { style.border_right_color  = c; } }
        "border-bottom-color" => { if let Some(c) = parse_color(v) { style.border_bottom_color = c; } }
        "border-left-color"   => { if let Some(c) = parse_color(v) { style.border_left_color   = c; } }
        "top"    => { style.top    = parse_length(v); }
        "right"  => { style.right  = parse_length(v); }
        "bottom" => { style.bottom = parse_length(v); }
        "left"   => { style.left   = parse_length(v); }

        "color"            => { if let Some(c) = parse_color(v) { style.color = c; } }
        "background-color" => { if let Some(c) = parse_color(v) { style.background_color = c; } }
        "background"       => {
            // Handle gradient functions first (they contain spaces and commas)
            if v.contains("gradient") {
                apply_gradient(style, v);
                return;
            }
            // Parse background shorthand: color, image url(), position [/ size], repeat
            // Split on "/" to separate position from size
            let (pos_part, size_part) = if let Some(slash) = v.find(" / ") {
                (&v[..slash], Some(&v[slash+3..]))
            } else {
                (v, None)
            };
            // Process position/size part first
            if let Some(size_str) = size_part {
                // size_str may have tokens after size (e.g. "cover no-repeat")
                let size_tok: &str = size_str.split_whitespace().next().unwrap_or("auto");
                match size_tok {
                    "cover"   => style.background_size = BackgroundSize::Cover,
                    "contain" => style.background_size = BackgroundSize::Contain,
                    _         => {
                        style.background_size = BackgroundSize::Explicit;
                        style.background_size_w = parse_length(size_tok);
                        style.background_size_h = CssLength::Auto;
                    }
                }
                // Any remaining tokens in size_part (like repeat keywords)
                for tok in size_str.split_whitespace().skip(1) {
                    match tok {
                        "no-repeat" => style.background_repeat = BackgroundRepeat::NoRepeat,
                        "repeat-x"  => style.background_repeat = BackgroundRepeat::RepeatX,
                        "repeat-y"  => style.background_repeat = BackgroundRepeat::RepeatY,
                        "repeat"    => style.background_repeat = BackgroundRepeat::Repeat,
                        _ => {}
                    }
                }
            }
            // Parse position part tokens (and remaining repeat/color/url)
            let mut pos_tokens: Vec<&str> = Vec::new();
            for token in pos_part.split_whitespace() {
                match token {
                    "no-repeat"  => { style.background_repeat = BackgroundRepeat::NoRepeat; }
                    "repeat-x"   => { style.background_repeat = BackgroundRepeat::RepeatX; }
                    "repeat-y"   => { style.background_repeat = BackgroundRepeat::RepeatY; }
                    "repeat"     => { style.background_repeat = BackgroundRepeat::Repeat; }
                    "left" | "center" | "right" | "top" | "bottom" => {
                        pos_tokens.push(token);
                    }
                    _ if token.starts_with("url(") => {
                        let url = token.trim_start_matches("url(").trim_end_matches(')')
                            .trim_matches('"').trim_matches('\'');
                        style.background_image_url = url.to_string();
                    }
                    _ => {
                        if let Some(c) = parse_color(token) {
                            style.background_color = c;
                        } else if token.ends_with('%') || token.ends_with("px") || token.ends_with("em") {
                            pos_tokens.push(token);
                        }
                    }
                }
            }
            // Assign position tokens
            if !pos_tokens.is_empty() {
                // Separate x/y: left/right/center-h go to x, top/bottom/center-v go to y
                let mut x_set = false;
                let mut y_set = false;
                for tok in &pos_tokens {
                    match *tok {
                        "left"   => { style.background_position_x = CssLength::Percent(0.0);   x_set = true; }
                        "right"  => { style.background_position_x = CssLength::Percent(100.0); x_set = true; }
                        "top"    => { style.background_position_y = CssLength::Percent(0.0);   y_set = true; }
                        "bottom" => { style.background_position_y = CssLength::Percent(100.0); y_set = true; }
                        "center" => {
                            if !x_set { style.background_position_x = CssLength::Percent(50.0); x_set = true; }
                            else if !y_set { style.background_position_y = CssLength::Percent(50.0); y_set = true; }
                        }
                        other => {
                            let l = parse_length(other);
                            if !x_set { style.background_position_x = l; x_set = true; }
                            else if !y_set { style.background_position_y = l; }
                        }
                    }
                }
                // If only one positional token was given, default the other to center
                if x_set && !y_set { style.background_position_y = CssLength::Percent(50.0); }
            }
        }

        "font-family" => { style.font_family = v.trim_matches('"').trim_matches('\'').to_string(); }
        "font-size"   => { style.font_size   = parse_font_size(v); }
        "font-weight" => {
            style.font_weight = match v {
                "normal"  => FontWeight::Normal,
                "bold"    => FontWeight::Bold,
                "bolder"  => FontWeight::Value(700),
                "lighter" => FontWeight::Value(300),
                _         => v.parse::<u16>().map(FontWeight::Value).unwrap_or(FontWeight::Normal),
            };
        }
        "font-style" => {
            style.font_style = match v {
                "italic"  => FontStyle::Italic,
                "oblique" => FontStyle::Oblique,
                _         => FontStyle::Normal,
            };
        }
        "font" => apply_font_shorthand(style, v),

        "line-height"    => { style.line_height    = parse_line_height(v); }
        "letter-spacing" => { style.letter_spacing = parse_length(v); }
        "word-spacing"   => { style.word_spacing   = parse_length(v); }
        "text-align"     => {
            style.text_align = match v {
                "right"   => TextAlign::Right,
                "center"  => TextAlign::Center,
                "justify" => TextAlign::Justify,
                "end"     => TextAlign::End,
                "start"   => TextAlign::Start,
                _         => TextAlign::Left,
            };
        }
        "vertical-align" => {
            style.vertical_align = match v {
                "top"         => VerticalAlign::Top,
                "middle"      => VerticalAlign::Middle,
                "bottom"      => VerticalAlign::Bottom,
                "text-top"    => VerticalAlign::TextTop,
                "text-bottom" => VerticalAlign::TextBottom,
                "sub"         => VerticalAlign::Sub,
                "super"       => VerticalAlign::Super,
                _             => VerticalAlign::Baseline,
            };
        }
        "text-decoration" => {
            style.text_decoration.underline      = v.contains("underline");
            style.text_decoration.overline       = v.contains("overline");
            style.text_decoration.strikethrough  = v.contains("line-through");
        }
        "text-indent"    => { style.text_indent   = parse_length(v); }
        "white-space"    => {
            style.white_space = match v {
                "nowrap"   => WhiteSpace::Nowrap,
                "pre"      => WhiteSpace::Pre,
                "pre-wrap" => WhiteSpace::PreWrap,
                "pre-line" => WhiteSpace::PreLine,
                _          => WhiteSpace::Normal,
            };
        }
        "text-transform" => {
            style.text_transform = match v {
                "uppercase"  => TextTransform::Uppercase,
                "lowercase"  => TextTransform::Lowercase,
                "capitalize" => TextTransform::Capitalize,
                _            => TextTransform::None,
            };
        }
        "word-break" => {
            style.word_break = match v {
                "break-all"  => WordBreak::BreakAll,
                "keep-all"   => WordBreak::KeepAll,
                "break-word" => WordBreak::BreakWord,
                _            => WordBreak::Normal,
            };
        }
        "overflow-wrap" | "word-wrap" => {
            style.overflow_wrap = match v {
                "break-word" => OverflowWrap::BreakWord,
                "anywhere"   => OverflowWrap::Anywhere,
                _            => OverflowWrap::Normal,
            };
        }
        "direction" => {
            style.direction = match v {
                "rtl" => Direction::RTL,
                _     => Direction::LTR,
            };
        }

        "list-style-type" => {
            style.list_style_type = match v {
                "none"         => ListStyleType::None,
                "disc"         => ListStyleType::Disc,
                "circle"       => ListStyleType::Circle,
                "square"       => ListStyleType::Square,
                "decimal"      => ListStyleType::Decimal,
                "lower-alpha"  => ListStyleType::LowerAlpha,
                "upper-alpha"  => ListStyleType::UpperAlpha,
                "lower-roman"  => ListStyleType::LowerRoman,
                "upper-roman"  => ListStyleType::UpperRoman,
                _              => ListStyleType::None,
            };
        }
        "list-style-position" => {
            style.list_style_position = if v == "inside" {
                ListStylePosition::Inside
            } else {
                ListStylePosition::Outside
            };
        }

        // Flexbox
        "flex-direction" => {
            style.flex_direction = match v {
                "row-reverse"    => FlexDirection::RowReverse,
                "column"         => FlexDirection::Column,
                "column-reverse" => FlexDirection::ColumnReverse,
                _                => FlexDirection::Row,
            };
        }
        "flex-wrap" => {
            style.flex_wrap = match v {
                "wrap"         => FlexWrap::Wrap,
                "wrap-reverse" => FlexWrap::WrapReverse,
                _              => FlexWrap::Nowrap,
            };
        }
        "justify-content" => {
            style.justify_content = match v {
                "flex-end" | "end"   => JustifyContent::FlexEnd,
                "center"             => JustifyContent::Center,
                "space-between"      => JustifyContent::SpaceBetween,
                "space-around"       => JustifyContent::SpaceAround,
                "space-evenly"       => JustifyContent::SpaceEvenly,
                _                    => JustifyContent::FlexStart,
            };
        }
        "align-items" => {
            style.align_items = match v {
                "flex-start" | "start" | "self-start" => AlignItems::FlexStart,
                "flex-end"   | "end"   | "self-end"   => AlignItems::FlexEnd,
                "center"     => AlignItems::Center,
                "baseline"   => AlignItems::Baseline,
                _            => AlignItems::Stretch,
            };
        }
        "align-self" => {
            style.align_self = match v {
                "flex-start" | "start" | "self-start" => AlignSelf::FlexStart,
                "flex-end"   | "end"   | "self-end"   => AlignSelf::FlexEnd,
                "center"     => AlignSelf::Center,
                "baseline"   => AlignSelf::Baseline,
                "stretch"    => AlignSelf::Stretch,
                _            => AlignSelf::Auto,
            };
        }
        "flex-grow"   => { style.flex_grow   = v.parse().unwrap_or(0.0); }
        "flex-shrink" => { style.flex_shrink = v.parse().unwrap_or(1.0); }
        "flex-basis"  => { style.flex_basis  = parse_length(v); }
        "order"       => { style.order       = v.parse().unwrap_or(0); }
        "gap"         => {
            let g = parse_length(v);
            style.gap = g;
            style.row_gap = g;
            style.column_gap = g;
        }
        "row-gap"    => { style.row_gap    = parse_length(v); }
        "column-gap" => { style.column_gap = parse_length(v); }

        "box-sizing" => {
            style.box_sizing = match v {
                "border-box" => BoxSizing::BorderBox,
                _            => BoxSizing::ContentBox,
            };
        }
        "align-content" => {
            style.align_content = match v {
                "flex-start"   => AlignContent::FlexStart,
                "flex-end"     => AlignContent::FlexEnd,
                "center"       => AlignContent::Center,
                "space-between"=> AlignContent::SpaceBetween,
                "space-around" => AlignContent::SpaceAround,
                "space-evenly" => AlignContent::SpaceEvenly,
                _              => AlignContent::Stretch,
            };
        }
        "grid-auto-flow" => {
            style.grid_auto_flow = match v {
                "column"        => GridAutoFlow::Column,
                "row dense"     => GridAutoFlow::RowDense,
                "column dense"  => GridAutoFlow::ColumnDense,
                _               => GridAutoFlow::Row,
            };
        }
        "grid-template-columns" => {
            style.grid_template_columns = parse_track_list(v, &mut style.auto_repeat_columns);
        }
        "grid-template-rows" => {
            let mut dummy = Vec::new();
            style.grid_template_rows = parse_track_list(v, &mut dummy);
        }
        "grid-auto-columns" => {
            style.grid_auto_columns = parse_single_track(v);
        }
        "grid-auto-rows" => {
            style.grid_auto_rows = parse_single_track(v);
        }
        "grid-template-areas" => {
            style.grid_template_areas = parse_grid_template_areas(v);
        }
        "grid-column" => {
            // "start / end" or "span N"
            if let Some(slash) = v.find('/') {
                style.grid_column_start = parse_grid_line(v[..slash].trim());
                style.grid_column_end   = parse_grid_line(v[slash+1..].trim());
            } else {
                style.grid_column_start = parse_grid_line(v);
                style.grid_column_end   = 0;
            }
        }
        "grid-row" => {
            if let Some(slash) = v.find('/') {
                style.grid_row_start = parse_grid_line(v[..slash].trim());
                style.grid_row_end   = parse_grid_line(v[slash+1..].trim());
            } else {
                style.grid_row_start = parse_grid_line(v);
                style.grid_row_end   = 0;
            }
        }
        "grid-column-start" => { style.grid_column_start = parse_grid_line(v); }
        "grid-column-end"   => { style.grid_column_end   = parse_grid_line(v); }
        "grid-row-start"    => { style.grid_row_start    = parse_grid_line(v); }
        "grid-row-end"      => { style.grid_row_end      = parse_grid_line(v); }
        "grid-area" => {
            // "1 / 2 / 3 / 4" → row-start / col-start / row-end / col-end
            let parts: Vec<&str> = v.splitn(4, '/').collect();
            if parts.len() == 4 {
                let rs = parse_grid_line(parts[0].trim());
                let cs = parse_grid_line(parts[1].trim());
                let re = parse_grid_line(parts[2].trim());
                let ce = parse_grid_line(parts[3].trim());
                if rs != 0 || cs != 0 || re != 0 || ce != 0 {
                    style.grid_row_start    = rs;
                    style.grid_column_start = cs;
                    style.grid_row_end      = re;
                    style.grid_column_end   = ce;
                } else {
                    style.grid_area = v.to_string();
                }
            } else {
                style.grid_area = v.to_string();
            }
        }
        "justify-items" => {
            style.justify_items = match v {
                "flex-start" | "start" => AlignItems::FlexStart,
                "flex-end"   | "end"   => AlignItems::FlexEnd,
                "center"               => AlignItems::Center,
                "baseline"             => AlignItems::Baseline,
                _                      => AlignItems::Stretch,
            };
        }
        "justify-self" => {
            style.justify_self = match v {
                "flex-start" | "start" => AlignSelf::FlexStart,
                "flex-end"   | "end"   => AlignSelf::FlexEnd,
                "center"               => AlignSelf::Center,
                "baseline"             => AlignSelf::Baseline,
                "stretch"              => AlignSelf::Stretch,
                _                      => AlignSelf::Auto,
            };
        }

        "opacity"     => { let op: f32 = v.parse().unwrap_or(1.0); style.opacity = op.max(0.0).min(1.0); }
        "visibility"  => { style.visibility = v != "hidden" && v != "collapse"; }

        // ── Per-side border shorthands ──────────────────────────────────────
        "border-top"    => apply_border_side_shorthand(v, &mut style.border_top_width,    &mut style.border_top_style,    &mut style.border_top_color),
        "border-right"  => apply_border_side_shorthand(v, &mut style.border_right_width,  &mut style.border_right_style,  &mut style.border_right_color),
        "border-bottom" => apply_border_side_shorthand(v, &mut style.border_bottom_width, &mut style.border_bottom_style, &mut style.border_bottom_color),
        "border-left"   => apply_border_side_shorthand(v, &mut style.border_left_width,   &mut style.border_left_style,   &mut style.border_left_color),

        // ── Per-corner border radius ────────────────────────────────────────
        "border-top-left-radius"     => { style.border_top_left_radius     = parse_length(v); style.border_radius = style.border_top_left_radius; }
        "border-top-right-radius"    => { style.border_top_right_radius    = parse_length(v); }
        "border-bottom-left-radius"  => { style.border_bottom_left_radius  = parse_length(v); }
        "border-bottom-right-radius" => { style.border_bottom_right_radius = parse_length(v); }

        // ── Table ───────────────────────────────────────────────────────────
        "border-collapse"  => { style.border_collapse = v == "collapse"; }
        "border-spacing"   => {
            let parts: Vec<&str> = v.split_whitespace().collect();
            style.border_spacing_h = parse_length(parts.first().copied().unwrap_or("0"));
            style.border_spacing_v = parse_length(parts.get(1).copied().unwrap_or(parts.first().copied().unwrap_or("0")));
        }
        "caption-side"   => { style.caption_side       = if v == "bottom" { CaptionSide::Bottom } else { CaptionSide::Top }; }
        "empty-cells"    => { style.empty_cells_hide   = v == "hide"; }
        "table-layout"   => { style.table_layout_fixed = v == "fixed"; }
        "cellpadding"    => {
            style.cell_padding = parse_length(v);
        }
        "cellspacing"    => { style.border_spacing_h = parse_length(v); style.border_spacing_v = parse_length(v); }


        // ── Background ──────────────────────────────────────────────────────
        "background-image" => {
            if v.contains("gradient") {
                apply_gradient(style, v);
            } else if let Some(url) = extract_url(v) {
                style.background_image_url = url;
            }
        }
        "background-size" => {
            match v {
                "cover"   => { style.background_size = BackgroundSize::Cover; }
                "contain" => { style.background_size = BackgroundSize::Contain; }
                "auto"    => { style.background_size = BackgroundSize::Auto; }
                _ => {
                    style.background_size = BackgroundSize::Explicit;
                    let parts: Vec<&str> = v.split_whitespace().collect();
                    style.background_size_w = parse_length(parts.first().copied().unwrap_or("auto"));
                    style.background_size_h = if parts.len() >= 2 { parse_length(parts[1]) } else { CssLength::Auto };
                }
            }
        }
        "background-position" => {
            let parts: Vec<&str> = v.split_whitespace().collect();
            let x_str = parts.first().copied().unwrap_or("0%");
            style.background_position_x = match x_str {
                "left"   => CssLength::Percent(0.0),
                "center" => CssLength::Percent(50.0),
                "right"  => CssLength::Percent(100.0),
                _        => parse_length(x_str),
            };
            let y_str = parts.get(1).copied().unwrap_or("center");
            style.background_position_y = match y_str {
                "top"    => CssLength::Percent(0.0),
                "center" => CssLength::Percent(50.0),
                "bottom" => CssLength::Percent(100.0),
                _        => parse_length(y_str),
            };
        }
        "background-repeat" => {
            style.background_repeat = match v {
                "repeat"    => BackgroundRepeat::Repeat,
                "repeat-x"  => BackgroundRepeat::RepeatX,
                "repeat-y"  => BackgroundRepeat::RepeatY,
                "no-repeat" => BackgroundRepeat::NoRepeat,
                _           => BackgroundRepeat::Repeat,
            };
        }

        // ── Outline ─────────────────────────────────────────────────────────
        "outline" => {
            if v == "none" {
                style.outline_style = BorderStyle::None;
                style.outline_width = 0.0;
            } else {
                for tok in v.split_whitespace() {
                    match tok {
                        "solid"  => { style.outline_style = BorderStyle::Solid; }
                        "dashed" => { style.outline_style = BorderStyle::Dashed; }
                        "dotted" => { style.outline_style = BorderStyle::Dotted; }
                        "double" => { style.outline_style = BorderStyle::Double; }
                        "inset"  => { style.outline_style = BorderStyle::Inset; }
                        "outset" => { style.outline_style = BorderStyle::Outset; }
                        "groove" => { style.outline_style = BorderStyle::Groove; }
                        "ridge"  => { style.outline_style = BorderStyle::Ridge; }
                        "none"   => { style.outline_style = BorderStyle::None; }
                        _ => {
                            if let CssLength::Px(w) = parse_length(tok) {
                                style.outline_width = w;
                            } else if let Some(c) = parse_color(tok) {
                                style.outline_color = c;
                            }
                        }
                    }
                }
            }
        }
        "outline-style"  => { style.outline_style  = parse_border_style(v); }
        "outline-color"  => { if let Some(c) = parse_color(v) { style.outline_color = c; } }
        "outline-width"  => { if let CssLength::Px(w) = parse_length(v) { style.outline_width = w; } }
        "outline-offset" => { if let CssLength::Px(w) = parse_length(v) { style.outline_offset = w; } }

        // ── Text & content ──────────────────────────────────────────────────
        "text-overflow"    => { style.text_overflow = if v == "ellipsis" { TextOverflow::Ellipsis } else { TextOverflow::Clip }; }
        "text-shadow"      => {
            if v == "none" {
                style.text_shadow = None;
            } else {
                let ts = parse_shadow_value(v);
                style.text_shadow = Some(TextShadow { offset_x: ts.0, offset_y: ts.1, blur: ts.2, color: ts.3 });
            }
        }
        "font-variant"     => { style.small_caps = v == "small-caps"; }
        "tab-size"         => { style.tab_size = v.parse().unwrap_or(8); }
        "hyphens"          => {
            style.hyphens = match v {
                "none"   => Hyphens::None,
                "manual" => Hyphens::Manual,
                "auto"   => Hyphens::Auto,
                _        => Hyphens::Manual,
            };
        }
        "widows"  => { if let Ok(n) = v.parse() { style.widows  = n; } }
        "orphans" => { if let Ok(n) = v.parse() { style.orphans = n; } }

        // ── Unicode-bidi & writing ───────────────────────────────────────────
        "unicode-bidi" => {
            style.unicode_bidi = match v {
                "normal"           => UnicodeBidi::Normal,
                "embed"            => UnicodeBidi::Embed,
                "bidi-override"    => UnicodeBidi::Override,
                "isolate"          => UnicodeBidi::Isolate,
                "isolate-override" => UnicodeBidi::IsolateOverride,
                "plaintext"        => UnicodeBidi::Plaintext,
                _                  => UnicodeBidi::Normal,
            };
        }
        "writing-mode" => {
            style.writing_mode = match v {
                "vertical-rl" => WritingMode::VerticalRL,
                "vertical-lr" => WritingMode::VerticalLR,
                _             => WritingMode::HorizontalTB,
            };
        }

        // ── Object fit ──────────────────────────────────────────────────────
        "object-fit" => {
            style.object_fit = match v {
                "contain"    => ObjectFit::Contain,
                "cover"      => ObjectFit::Cover,
                "none"       => ObjectFit::None,
                "scale-down" => ObjectFit::ScaleDown,
                _            => ObjectFit::Fill,
            };
        }

        // ── List style ──────────────────────────────────────────────────────
        "list-style" => {
            if v.contains("none")          { style.list_style_type = ListStyleType::None; }
            else if v.contains("disc")     { style.list_style_type = ListStyleType::Disc; }
            else if v.contains("circle")   { style.list_style_type = ListStyleType::Circle; }
            else if v.contains("square")   { style.list_style_type = ListStyleType::Square; }
            else if v.contains("decimal")  { style.list_style_type = ListStyleType::Decimal; }
        }
        "list-style-image" => {
            if v == "none" {
                style.list_style_image = String::new();
            } else if let Some(url) = extract_url(v) {
                style.list_style_image = url;
            }
        }

        // ── Flex shorthands ──────────────────────────────────────────────────
        "flex" => {
            match v {
                "none" => { style.flex_grow = 0.0; style.flex_shrink = 0.0; style.flex_basis = CssLength::Auto; }
                "auto" => { style.flex_grow = 1.0; style.flex_shrink = 1.0; style.flex_basis = CssLength::Auto; }
                _ => {
                    let toks: Vec<&str> = v.split_whitespace().collect();
                    if let Some(t0) = toks.first() { style.flex_grow = t0.parse().unwrap_or(0.0); }
                    if let Some(t1) = toks.get(1)  { style.flex_shrink = t1.parse().unwrap_or(1.0); }
                    else                            { style.flex_shrink = 1.0; style.flex_basis = CssLength::Px(0.0); }
                    if let Some(t2) = toks.get(2)  { style.flex_basis = parse_length(t2); }
                }
            }
        }
        "flex-flow" => {
            for tok in v.split_whitespace() {
                match tok {
                    "row"            => { style.flex_direction = FlexDirection::Row; }
                    "row-reverse"    => { style.flex_direction = FlexDirection::RowReverse; }
                    "column"         => { style.flex_direction = FlexDirection::Column; }
                    "column-reverse" => { style.flex_direction = FlexDirection::ColumnReverse; }
                    "nowrap"         => { style.flex_wrap = FlexWrap::Nowrap; }
                    "wrap"           => { style.flex_wrap = FlexWrap::Wrap; }
                    "wrap-reverse"   => { style.flex_wrap = FlexWrap::WrapReverse; }
                    _ => {}
                }
            }
        }

        // ── Grid shorthands ──────────────────────────────────────────────────
        "grid" | "grid-template" => {
            if v == "none" {
                style.grid_template_rows.clear(); style.grid_template_columns.clear();
            } else if let Some(slash) = v.find('/') {
                let rows_part = v[..slash].trim();
                let cols_part = v[slash+1..].trim();
                apply_property(style, "grid-template-rows", rows_part);
                apply_property(style, "grid-template-columns", cols_part);
            }
        }

        // ── Logical properties (block/inline) ───────────────────────────────
        "margin-block"        => { let m = parse_length(v); style.margin_top = m; style.margin_bottom = m; }
        "margin-block-start"  => { style.margin_top    = parse_length(v); }
        "margin-block-end"    => { style.margin_bottom = parse_length(v); }
        "margin-inline"       => { let m = parse_length(v); style.margin_left = m; style.margin_right = m; }
        "margin-inline-start" => { style.margin_left   = parse_length(v); }
        "margin-inline-end"   => { style.margin_right  = parse_length(v); }

        "padding-block"        => { let p = parse_length(v); style.padding_top = p; style.padding_bottom = p; }
        "padding-block-start"  => { style.padding_top    = parse_length(v); }
        "padding-block-end"    => { style.padding_bottom = parse_length(v); }
        "padding-inline"       => { let p = parse_length(v); style.padding_left = p; style.padding_right = p; }
        "padding-inline-start" => { style.padding_left  = parse_length(v); }
        "padding-inline-end"   => { style.padding_right = parse_length(v); }

        "inset-block-start"  => { style.top    = parse_length(v); }
        "inset-block-end"    => { style.bottom = parse_length(v); }
        "inset-inline-start" => {
            if style.direction == Direction::RTL { style.right = parse_length(v); }
            else                                 { style.left  = parse_length(v); }
        }
        "inset-inline-end" => {
            if style.direction == Direction::RTL { style.left  = parse_length(v); }
            else                                 { style.right = parse_length(v); }
        }

        // ── Place shorthands ─────────────────────────────────────────────────
        "place-self" => {
            let parts: Vec<&str> = v.splitn(2, ' ').collect();
            apply_property(style, "align-self",   parts.first().copied().unwrap_or(v));
            apply_property(style, "justify-self", parts.get(1).copied().unwrap_or(v));
        }
        "place-items" => {
            let parts: Vec<&str> = v.splitn(2, ' ').collect();
            apply_property(style, "align-items",   parts.first().copied().unwrap_or(v));
            apply_property(style, "justify-items", parts.get(1).copied().unwrap_or(v));
        }
        "place-content" => {
            let parts: Vec<&str> = v.splitn(2, ' ').collect();
            apply_property(style, "align-content",   parts.first().copied().unwrap_or(v));
            apply_property(style, "justify-content", parts.get(1).copied().unwrap_or(v));
        }

        // ── Break ────────────────────────────────────────────────────────────
        "break-before" | "page-break-before" => {
            style.break_before = match v {
                "always" | "page" => BreakValue::Always,
                "avoid"           => BreakValue::Avoid,
                "left"            => BreakValue::Left,
                "right"           => BreakValue::Right,
                _                 => BreakValue::Auto,
            };
        }
        "break-after" | "page-break-after" => {
            style.break_after = match v {
                "always" | "page" => BreakValue::Always,
                "avoid"           => BreakValue::Avoid,
                "left"            => BreakValue::Left,
                "right"           => BreakValue::Right,
                _                 => BreakValue::Auto,
            };
        }
        "break-inside" | "page-break-inside" => {
            style.break_inside = if v == "avoid" { BreakInside::Avoid } else { BreakInside::Auto };
        }

        // ── Box shadow ───────────────────────────────────────────────────────
        "box-shadow" => {
            if v == "none" {
                style.box_shadow = None;
            } else {
                let (ox, oy, blur, color) = parse_shadow_value(v);
                // parse spread: 4th numeric token
                let toks: Vec<&str> = v.split_whitespace().collect();
                let nums: Vec<f32> = toks.iter()
                    .filter_map(|t| { let c = t.trim_start_matches('-').chars().next()?; if c.is_ascii_digit() || c == '.' { t.trim_end_matches("px").parse().ok() } else { None } })
                    .collect();
                let spread = nums.get(3).copied().unwrap_or(0.0);
                let inset = v.contains("inset");
                style.box_shadow = Some(BoxShadow { offset_x: ox, offset_y: oy, blur, spread, color, inset });
            }
        }

        // ── Cursor ────────────────────────────────────────────────────────────
        "cursor" => {
            style.cursor = match v {
                "default"      => CSSCursor::Default,
                "pointer"      => CSSCursor::Pointer,
                "text"         => CSSCursor::Text,
                "move"         => CSSCursor::Move,
                "crosshair"    => CSSCursor::Crosshair,
                "wait"         => CSSCursor::Wait,
                "help"         => CSSCursor::Help,
                "not-allowed"  => CSSCursor::NotAllowed,
                "grab"         => CSSCursor::Grab,
                "grabbing"     => CSSCursor::Grabbing,
                "col-resize"   => CSSCursor::ColResize,
                "row-resize"   => CSSCursor::RowResize,
                "n-resize"     => CSSCursor::NResize,
                "e-resize"     => CSSCursor::EResize,
                "s-resize"     => CSSCursor::SResize,
                "w-resize"     => CSSCursor::WResize,
                "ne-resize"    => CSSCursor::NEResize,
                "nw-resize"    => CSSCursor::NWResize,
                "se-resize"    => CSSCursor::SEResize,
                "sw-resize"    => CSSCursor::SWResize,
                "none"         => CSSCursor::None,
                _              => CSSCursor::Auto,
            };
        }

        // ── Pointer events ────────────────────────────────────────────────────
        "pointer-events" => {
            style.pointer_events = match v {
                "none"           => PointerEvents::None,
                "visiblePainted" => PointerEvents::VisiblePainted,
                "visibleFill"    => PointerEvents::VisibleFill,
                "visibleStroke"  => PointerEvents::VisibleStroke,
                "visible"        => PointerEvents::Visible,
                "painted"        => PointerEvents::Painted,
                "fill"           => PointerEvents::Fill,
                "stroke"         => PointerEvents::Stroke,
                "all"            => PointerEvents::All,
                _                => PointerEvents::Auto,
            };
        }

        // ── Scrollbar & caret ─────────────────────────────────────────────────
        "scrollbar-color" => {
            if v != "auto" {
                // find space not inside parens: "thumb track"
                let sp = find_split_space(v);
                if let Some(idx) = sp {
                    let thumb = v[..idx].trim();
                    let track = v[idx+1..].trim();
                    style.scrollbar_thumb_color = parse_color(thumb);
                    style.scrollbar_track_color = parse_color(track);
                }
            }
        }
        "caret-color" => {
            style.caret_color = if v == "auto" { None } else { parse_color(v) };
        }

        // ── Quotes ────────────────────────────────────────────────────────────
        "quotes" => {
            style.quotes.clear();
            if v != "none" && v != "auto" {
                let bytes = v.as_bytes();
                let mut i = 0;
                while i < bytes.len() {
                    if bytes[i] == b'"' || bytes[i] == b'\'' {
                        let q = bytes[i]; i += 1;
                        let start = i;
                        while i < bytes.len() && bytes[i] != q { i += 1; }
                        style.quotes.push(v[start..i].to_string());
                        if i < bytes.len() { i += 1; }
                    } else { i += 1; }
                }
            }
        }

        // ── Container queries ─────────────────────────────────────────────────
        "container-type" => {
            style.container_type = match v {
                "size"        => ContainerType::Size,
                "inline-size" => ContainerType::InlineSize,
                _             => ContainerType::Normal,
            };
        }
        "container-name" => { style.container_name = v.to_string(); }
        "container" => {
            if let Some(slash) = v.find('/') {
                style.container_name = v[..slash].trim().to_string();
                apply_property(style, "container-type", v[slash+1..].trim());
            } else {
                match v {
                    "size" | "inline-size" => apply_property(style, "container-type", v),
                    _ => style.container_name = v.to_string(),
                }
            }
        }

        // ── Hover colors ──────────────────────────────────────────────────────
        "hover-color"            => { style.hover_color            = parse_color(v); }
        "hover-background-color" => { style.hover_background_color = parse_color(v); }

        // ── Clip path ─────────────────────────────────────────────────────────
        "clip-path" => {
            if v == "none" {
                style.clip_path = ClipPath::default();
            } else if v.starts_with("inset(") {
                let inner = v[6..v.len().saturating_sub(1)].trim();
                style.clip_path = ClipPath::default();
                style.clip_path.kind = ClipPathKind::Inset;
                let pts: Vec<&str> = inner.split_whitespace().collect();
                style.clip_path.inset_top    = parse_length(pts.first().copied().unwrap_or("0"));
                style.clip_path.inset_right  = parse_length(pts.get(1).copied().unwrap_or(pts.first().copied().unwrap_or("0")));
                style.clip_path.inset_bottom = parse_length(pts.get(2).copied().unwrap_or(pts.first().copied().unwrap_or("0")));
                style.clip_path.inset_left   = parse_length(pts.get(3).copied().unwrap_or(pts.get(1).copied().unwrap_or(pts.first().copied().unwrap_or("0"))));
            } else if v.starts_with("circle(") {
                let inner = v[7..v.len().saturating_sub(1)].trim();
                style.clip_path = ClipPath::default();
                style.clip_path.kind = ClipPathKind::Circle;
                if let Some(at) = inner.find(" at ") {
                    style.clip_path.circle_radius = parse_length(&inner[..at]);
                    let center: Vec<&str> = inner[at+4..].split_whitespace().collect();
                    style.clip_path.center_x = parse_length(center.first().copied().unwrap_or("50%"));
                    style.clip_path.center_y = parse_length(center.get(1).copied().unwrap_or(center.first().copied().unwrap_or("50%")));
                } else {
                    style.clip_path.circle_radius = parse_length(inner);
                    style.clip_path.center_x = CssLength::Percent(50.0);
                    style.clip_path.center_y = CssLength::Percent(50.0);
                }
            } else if v.starts_with("ellipse(") {
                let inner = v[8..v.len().saturating_sub(1)].trim();
                style.clip_path = ClipPath::default();
                style.clip_path.kind = ClipPathKind::Ellipse;
                let (radii, center) = if let Some(at) = inner.find(" at ") {
                    (&inner[..at], Some(&inner[at+4..]))
                } else { (inner, None) };
                let rv: Vec<&str> = radii.split_whitespace().collect();
                style.clip_path.ellipse_rx = parse_length(rv.first().copied().unwrap_or("50%"));
                style.clip_path.ellipse_ry = parse_length(rv.get(1).copied().unwrap_or(rv.first().copied().unwrap_or("50%")));
                if let Some(c) = center {
                    let cv: Vec<&str> = c.split_whitespace().collect();
                    style.clip_path.center_x = parse_length(cv.first().copied().unwrap_or("50%"));
                    style.clip_path.center_y = parse_length(cv.get(1).copied().unwrap_or(cv.first().copied().unwrap_or("50%")));
                } else {
                    style.clip_path.center_x = CssLength::Percent(50.0);
                    style.clip_path.center_y = CssLength::Percent(50.0);
                }
            } else if v.starts_with("polygon(") {
                let inner = v[8..v.len().saturating_sub(1)].trim();
                style.clip_path = ClipPath::default();
                style.clip_path.kind = ClipPathKind::Polygon;
                for pair in inner.split(',') {
                    let pts: Vec<&str> = pair.trim().split_whitespace().collect();
                    if pts.len() >= 2 {
                        style.clip_path.points.push((parse_length(pts[0]), parse_length(pts[1])));
                    }
                }
            }
        }

        // ── Object position ──────────────────────────────────────────────────
        "object-position" => {
            let parts: Vec<&str> = v.split_whitespace().collect();
            style.object_position_x = match parts.first().copied().unwrap_or("50%") {
                "left"   => CssLength::Percent(0.0),
                "center" => CssLength::Percent(50.0),
                "right"  => CssLength::Percent(100.0),
                s        => parse_length(s),
            };
            style.object_position_y = match parts.get(1).copied().unwrap_or("50%") {
                "top"    => CssLength::Percent(0.0),
                "center" => CssLength::Percent(50.0),
                "bottom" => CssLength::Percent(100.0),
                s        => parse_length(s),
            };
        }

        // ── Aspect ratio ──────────────────────────────────────────────────────
        "aspect-ratio" => {
            if v == "auto" {
                style.aspect_ratio = None;
            } else if let Some(slash) = v.find('/') {
                let w: f32 = v[..slash].trim().parse().unwrap_or(1.0);
                let h: f32 = v[slash+1..].trim().parse().unwrap_or(1.0);
                if h > 0.0 { style.aspect_ratio = Some(w / h); }
            } else if let Ok(n) = v.trim().parse::<f32>() {
                if n > 0.0 { style.aspect_ratio = Some(n); }
            }
        }

        // ── Text decoration sub-properties ───────────────────────────────────
        "text-decoration-line" => {
            style.text_decoration.underline     = v.contains("underline");
            style.text_decoration.overline      = v.contains("overline");
            style.text_decoration.strikethrough = v.contains("line-through");
        }
        "text-decoration-color" => {
            style.text_decoration_color = parse_color(v);
        }
        "text-decoration-style" => {
            style.text_decoration_style = match v {
                "double" => TextDecorationStyle::Double,
                "dotted" => TextDecorationStyle::Dotted,
                "dashed" => TextDecorationStyle::Dashed,
                "wavy"   => TextDecorationStyle::Wavy,
                _        => TextDecorationStyle::Solid,
            };
        }
        "text-decoration-thickness" | "text-underline-offset" => {
            // Accepted but not yet used in renderer
        }

        // ── User interaction ──────────────────────────────────────────────────
        "user-select" | "-webkit-user-select" | "-moz-user-select" => {
            style.user_select = match v {
                "none"    => UserSelect::None,
                "text"    => UserSelect::Text,
                "all"     => UserSelect::All,
                "contain" => UserSelect::Contain,
                _         => UserSelect::Auto,
            };
        }
        "resize" => {
            style.resize = match v {
                "both"       => Resize::Both,
                "horizontal" => Resize::Horizontal,
                "vertical"   => Resize::Vertical,
                _            => Resize::None,
            };
        }

        // ── Background extras ─────────────────────────────────────────────────
        "background-clip" | "-webkit-background-clip" => {
            style.background_clip = match v {
                "padding-box" => BackgroundClip::PaddingBox,
                "content-box" => BackgroundClip::ContentBox,
                "text"        => BackgroundClip::Text,
                _             => BackgroundClip::BorderBox,
            };
        }
        "background-origin" => {
            style.background_origin = match v {
                "border-box"  => BackgroundClip::BorderBox,
                "content-box" => BackgroundClip::ContentBox,
                _             => BackgroundClip::PaddingBox,
            };
        }
        "background-attachment" => {
            style.background_attachment = match v {
                "fixed" => BackgroundAttachment::Fixed,
                "local" => BackgroundAttachment::Local,
                _       => BackgroundAttachment::Scroll,
            };
        }

        // ── Multi-column ─────────────────────────────────────────────────────
        "column-count" => {
            style.column_count = if v == "auto" { None } else { v.parse().ok() };
        }
        "column-width" => {
            style.column_width = parse_length(v);
        }
        "columns" => {
            // "auto auto" | "2" | "200px" | "2 200px"
            for tok in v.split_whitespace() {
                if let Ok(n) = tok.parse::<i32>() {
                    style.column_count = Some(n);
                } else {
                    style.column_width = parse_length(tok);
                }
            }
        }
        "column-rule" => {
            apply_border_side_shorthand(v,
                &mut style.column_rule_width,
                &mut style.column_rule_style,
                &mut style.column_rule_color);
        }
        "column-rule-width" => { style.column_rule_width = parse_length(v); }
        "column-rule-style" => { style.column_rule_style = parse_border_style(v); }
        "column-rule-color" => { if let Some(c) = parse_color(v) { style.column_rule_color = c; } }
        "column-fill" => { style.column_fill = v == "balance"; }
        "column-span" => {} // not yet implemented

        // ── Transform / filter ────────────────────────────────────────────────
        "transform" => { style.transform = v.to_string(); }
        "transform-origin" | "transform-box" | "transform-style" | "perspective-origin" | "perspective" | "backface-visibility" => {
            // Accepted but not implemented
        }
        "filter"          => { style.filter          = v.to_string(); }
        "backdrop-filter" => { style.backdrop_filter = v.to_string(); }

        // ── Transition / animation ────────────────────────────────────────────
        "transition" => { style.transition = v.to_string(); }
        "transition-property" | "transition-duration" | "transition-timing-function" | "transition-delay" => {
            // Sub-properties stored collectively in transition string
        }
        "animation" => { style.animation = v.to_string(); }
        "animation-name" | "animation-duration" | "animation-timing-function"
        | "animation-delay" | "animation-iteration-count" | "animation-direction"
        | "animation-fill-mode" | "animation-play-state" => {
            // Sub-properties — not fully implemented
        }
        "will-change" => { style.will_change = v.to_string(); }

        // ── Misc ──────────────────────────────────────────────────────────────
        "scroll-behavior" => {
            style.scroll_behavior = if v == "smooth" { ScrollBehavior::Smooth } else { ScrollBehavior::Auto };
        }
        "overscroll-behavior" | "overscroll-behavior-x" | "overscroll-behavior-y" => {
            // Accepted but not implemented
        }
        "isolation" => {
            style.isolation = v == "isolate";
        }
        "mix-blend-mode" => {
            style.mix_blend_mode = match v {
                "multiply"    => MixBlendMode::Multiply,
                "screen"      => MixBlendMode::Screen,
                "overlay"     => MixBlendMode::Overlay,
                "darken"      => MixBlendMode::Darken,
                "lighten"     => MixBlendMode::Lighten,
                "color-dodge" => MixBlendMode::ColorDodge,
                "color-burn"  => MixBlendMode::ColorBurn,
                "hard-light"  => MixBlendMode::HardLight,
                "soft-light"  => MixBlendMode::SoftLight,
                "difference"  => MixBlendMode::Difference,
                "exclusion"   => MixBlendMode::Exclusion,
                "hue"         => MixBlendMode::Hue,
                "saturation"  => MixBlendMode::Saturation,
                "color"       => MixBlendMode::Color,
                "luminosity"  => MixBlendMode::Luminosity,
                _             => MixBlendMode::Normal,
            };
        }

        // ── Counter ───────────────────────────────────────────────────────────
        "counter-reset" => {
            style.counter_reset = parse_counter_list(v);
        }
        "counter-increment" => {
            style.counter_increment = parse_counter_list(v);
        }
        "counter-set" => {
            // Same syntax as counter-reset
            style.counter_reset = parse_counter_list(v);
        }

        // ── Font extras ───────────────────────────────────────────────────────
        "font-stretch" | "-webkit-font-stretch" => {
            style.font_stretch = match v {
                "ultra-condensed"  => 50.0,
                "extra-condensed"  => 62.5,
                "condensed"        => 75.0,
                "semi-condensed"   => 87.5,
                "normal"           => 100.0,
                "semi-expanded"    => 112.5,
                "expanded"         => 125.0,
                "extra-expanded"   => 150.0,
                "ultra-expanded"   => 200.0,
                s if s.ends_with('%') => s[..s.len()-1].parse().unwrap_or(100.0),
                _                  => 100.0,
            };
        }

        // ── Inset shorthand ───────────────────────────────────────────────────
        "inset" => {
            apply_shorthand_4(v,
                &mut style.top, &mut style.right,
                &mut style.bottom, &mut style.left,
                parse_length);
        }
        "inset-block"  => { let l = parse_length(v); style.top    = l; style.bottom = l; }
        "inset-inline" => { let l = parse_length(v); style.left   = l; style.right  = l; }

        // ── border-radius shorthand (per-corner) ─────────────────────────────
        "border-radius" => {
            // Support "Xpx / Ypx" (elliptical corners) and up to 4 values
            let radii = if let Some(slash) = v.find('/') {
                v[..slash].trim()
            } else {
                v
            };
            let parts: Vec<&str> = radii.split_whitespace().collect();
            let tl = parse_length(parts.first().copied().unwrap_or("0"));
            let tr = parse_length(parts.get(1).copied().unwrap_or(parts.first().copied().unwrap_or("0")));
            let br = parse_length(parts.get(2).copied().unwrap_or(parts.first().copied().unwrap_or("0")));
            let bl = parse_length(parts.get(3).copied().unwrap_or(parts.get(1).copied().unwrap_or(parts.first().copied().unwrap_or("0"))));
            style.border_top_left_radius     = tl;
            style.border_top_right_radius    = tr;
            style.border_bottom_right_radius = br;
            style.border_bottom_left_radius  = bl;
            style.border_radius              = tl;
        }

        // ── Appearance ────────────────────────────────────────────────────────
        "appearance" | "-webkit-appearance" | "-moz-appearance" => {
            // Accepted, not implemented
        }

        // ── Color-scheme / accent-color ───────────────────────────────────────
        "color-scheme" | "forced-color-adjust" | "color-interpolation" | "color-rendering" => {}
        "accent-color" => {
            // Store as background fallback for form controls
            if let Some(c) = parse_color(v) { style.background_color = c; }
        }

        // ── Image rendering ───────────────────────────────────────────────────
        "image-rendering" | "image-orientation" => {}

        // ── Containment ───────────────────────────────────────────────────────
        "contain" | "content-visibility" => {}

        // ── Scroll snap ───────────────────────────────────────────────────────
        "scroll-snap-type" | "scroll-snap-align" | "scroll-snap-stop"
        | "scroll-padding" | "scroll-padding-top" | "scroll-padding-right"
        | "scroll-padding-bottom" | "scroll-padding-left"
        | "scroll-margin"  | "scroll-margin-top"  | "scroll-margin-right"
        | "scroll-margin-bottom" | "scroll-margin-left" => {}

        // ── Touch / interaction ───────────────────────────────────────────────
        "touch-action" | "-webkit-touch-callout" | "-webkit-tap-highlight-color" => {}

        _ => {
            // CSS custom property
            if prop.starts_with("--") {
                style.custom_props.insert(prop.to_string(), v.to_string());
            }
        }
    }
}

/// Resolve `var(--name)` and `var(--name, fallback)` references in a CSS value.
pub fn resolve_var_references(val: &str, variables: &HashMap<String, String>) -> String {
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
                out.push_str(resolved);
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
                return v[1..v.len()-1].to_string();
            }
            // Multiple tokens (e.g. '"foo" open-quote'): concatenate resolved parts
            let mut out = String::new();
            let mut rest = v;
            while !rest.is_empty() {
                rest = rest.trim_start();
                if rest.starts_with('"') || rest.starts_with('\'') {
                    let q = &rest[..1];
                    if let Some(end) = rest[1..].find(q) {
                        out.push_str(&rest[1..end+1]);
                        rest = &rest[end+2..];
                    } else {
                        out.push_str(&rest[1..]);
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
                        _ => {} // attr(), counter(), etc. — ignore for now
                    }
                    rest = &rest[end..];
                }
            }
            out
        }
    }
}

/// Parse CSS counter list: "name1 3 name2 name3 -1" → [(name1,3),(name2,1),(name3,-1)]
fn parse_counter_list(v: &str) -> Vec<(String, i32)> {
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

fn apply_shorthand_4<F: Fn(&str) -> CssLength>(
    v: &str,
    top: &mut CssLength, right: &mut CssLength,
    bottom: &mut CssLength, left: &mut CssLength,
    parse: F,
) {
    let parts: Vec<&str> = v.split_whitespace().collect();
    match parts.len() {
        1 => { let x = parse(parts[0]); *top = x; *right = x; *bottom = x; *left = x; }
        2 => { let tb = parse(parts[0]); let rl = parse(parts[1]); *top = tb; *bottom = tb; *right = rl; *left = rl; }
        3 => { *top = parse(parts[0]); let rl = parse(parts[1]); *right = rl; *left = rl; *bottom = parse(parts[2]); }
        4 => { *top = parse(parts[0]); *right = parse(parts[1]); *bottom = parse(parts[2]); *left = parse(parts[3]); }
        _ => {}
    }
}

fn apply_border_shorthand(style: &mut ComputedStyle, v: &str) {
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
                style.border_top_width    = w;
                style.border_right_width  = w;
                style.border_bottom_width = w;
                style.border_left_width   = w;
            }
        }
    }
}

fn apply_border_side_shorthand(
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

fn extract_url(v: &str) -> Option<String> {
    let lower = v.to_lowercase();
    let start = lower.find("url(")?;
    let inner = v[start + 4..].trim();
    let inner = inner.trim_start_matches('"').trim_start_matches('\'');
    let end = inner.find(|c| c == ')' || c == '"' || c == '\'')?;
    Some(inner[..end].to_string())
}

/// Parse a CSS shadow value: `offset_x offset_y [blur] [color]`
/// Returns (offset_x, offset_y, blur_radius, color).
fn parse_shadow_value(v: &str) -> (f32, f32, f32, Color) {
    let mut nums: Vec<f32> = Vec::new();
    let mut color = Color { r: 0, g: 0, b: 0, a: 255 };
    for tok in v.split_whitespace() {
        if let Some(c) = parse_color(tok) {
            color = c;
        } else if let Ok(n) = tok.trim_end_matches("px").parse::<f32>() {
            nums.push(n);
        }
    }
    let ox   = nums.first().copied().unwrap_or(0.0);
    let oy   = nums.get(1).copied().unwrap_or(0.0);
    let blur = nums.get(2).copied().unwrap_or(0.0);
    (ox, oy, blur, color)
}

/// Find the byte index of a space that is not nested inside parentheses.
fn find_split_space(v: &str) -> Option<usize> {
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

fn apply_gradient(style: &mut ComputedStyle, v: &str) {
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
        // Simplified: just parse color stops
        if let Some(paren) = v.find('(') {
            let inner = &v[paren + 1..];
            let inner = inner.trim_end_matches(')');
            style.gradient_stops.clear();
            let n_stops = inner.split(',').count().max(1) as f32;
            for (i, stop) in inner.split(',').enumerate() {
                let stop = stop.trim();
                if let Some(c) = parse_color(stop) {
                    let pos = i as f32 / (n_stops - 1.0).max(1.0);
                    style.gradient_stops.push(GradientStop { color: c, position: pos });
                }
            }
        }
    }
}

fn apply_font_shorthand(style: &mut ComputedStyle, v: &str) {
    // Very simplified: just look for px/em values and font names
    let parts: Vec<&str> = v.split_whitespace().collect();
    for part in &parts {
        if part.ends_with("px") || part.ends_with("em") || part.ends_with("rem") {
            style.font_size = parse_length(part);
        } else if *part == "bold" {
            style.font_weight = FontWeight::Bold;
        } else if *part == "italic" {
            style.font_style = FontStyle::Italic;
        }
    }
}

// ─── Value Parsers ────────────────────────────────────────────────────────────

pub fn parse_length(v: &str) -> CssLength {
    let v = v.trim();
    if v == "auto"       { return CssLength::Auto; }
    if v == "0"          { return CssLength::Zero; }
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

pub fn parse_length_or_none(v: &str) -> CssLength {
    if v == "none" { CssLength::None } else { parse_length(v) }
}

fn parse_font_size(v: &str) -> CssLength {
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

fn parse_line_height(v: &str) -> CssLength {
    if v == "normal" { return CssLength::Em(1.2); }
    // Unitless number: treat as em
    if let Ok(n) = v.parse::<f32>() { return CssLength::Em(n); }
    parse_length(v)
}

fn parse_overflow(v: &str) -> Overflow {
    match v {
        "hidden" => Overflow::Hidden,
        "scroll" => Overflow::Scroll,
        "auto"   => Overflow::Auto,
        _        => Overflow::Visible,
    }
}

fn parse_border_style(v: &str) -> BorderStyle {
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
        "yellow"         => Some(Color::rgb(255, 255,   0)),
        "lightyellow"    => Some(Color::rgb(255, 255, 224)),
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
        "lightyellow"    => Some(Color::rgb(255, 255, 224)),
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
/// Handles repeat(), minmax(), fr, px, %, auto, min-content, max-content.
/// auto_repeat_cols receives any auto-fill/auto-fit tracks.
pub fn parse_track_list(v: &str, auto_repeat_cols: &mut Vec<GridTrackSize>) -> Vec<GridTrackSize> {
    if v.is_empty() { return Vec::new(); }
    // Tokenize respecting parens
    let tokens = tokenize_track_list(v);
    let mut result = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i].trim();
        if t.starts_with("repeat(") || (i + 1 < tokens.len() && t == "repeat") {
            // Find the full repeat(...) span
            let repeat_str = if t.starts_with("repeat(") && t.ends_with(')') {
                t.to_string()
            } else {
                // shouldn't happen with our tokenizer but handle gracefully
                t.to_string()
            };
            let inner = repeat_str.trim_start_matches("repeat(").trim_end_matches(')');
            let comma = inner.find(',').unwrap_or(0);
            let count_str = inner[..comma].trim();
            let track_str = inner[comma+1..].trim();
            let track = parse_single_track(track_str);
            if count_str == "auto-fill" || count_str == "auto-fit" {
                // Store in auto_repeat_cols; actual expansion is done at layout time
                // Do NOT push a placeholder: result stays empty when only auto-fill
                auto_repeat_cols.push(track.clone());
            } else {
                let count = count_str.parse::<usize>().unwrap_or(1);
                for _ in 0..count {
                    result.push(track.clone());
                }
            }
        } else if !t.is_empty() && !t.starts_with('[') {
            // Skip line names like [col-start]
            result.push(parse_single_track(t));
        }
        i += 1;
    }
    result
}

/// Tokenize a track list, keeping repeat(...) as single tokens.
fn tokenize_track_list(v: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for ch in v.chars() {
        match ch {
            '(' => { depth += 1; current.push(ch); }
            ')' => {
                if depth > 0 { depth -= 1; }
                current.push(ch);
                if depth == 0 {
                    tokens.push(current.trim().to_string());
                    current = String::new();
                }
            }
            ' ' | '\t' | '\n' if depth == 0 => {
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

/// Parse a grid line value.
/// "auto" → 0, "3" → 3, "span 2" → -2 (negative = span)
pub fn parse_grid_line(v: &str) -> i32 {
    let v = v.trim();
    if v == "auto" { return 0; }
    if v.starts_with("span ") {
        let n: i32 = v[5..].trim().parse().unwrap_or(1);
        return -n;  // negative = span
    }
    v.parse::<i32>().unwrap_or(0)
}

// ─── CSS Cascade ─────────────────────────────────────────────────────────────

/// Apply a stylesheet to all boxes in the tree (cascade + inheritance).
pub fn apply_cascade(root: &mut crate::types::HtmlBox, stylesheet: &Stylesheet,
                     parent_style: Option<&ComputedStyle>, root_font_px: f32) {
    apply_cascade_inner(root, stylesheet, parent_style, root_font_px, &[], 0, 1);
}

fn apply_cascade_inner(
    root: &mut crate::types::HtmlBox,
    stylesheet: &Stylesheet,
    parent_style: Option<&ComputedStyle>,
    root_font_px: f32,
    ancestors: &[AncestorInfo],
    child_index: usize,
    sibling_count: usize,
) {
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
            "border" => {
                if let Ok(w) = val.parse::<f32>() {
                    let s = format!("{}px solid", w);
                    apply_property(&mut style, "border", &s);
                }
            }
            "dir" => match val.to_ascii_lowercase().as_str() {
                "rtl" => apply_property(&mut style, "direction", "rtl"),
                _     => apply_property(&mut style, "direction", "ltr"),
            },
            _ => {}
        }
    }

    // Apply UA / author stylesheet rules (after presentational attrs, before inline style)
    let mut matched: Vec<(u32, HashMap<String, String>)> = Vec::new();
    for rule in &stylesheet.rules {
        for sel in &rule.selectors {
            if sel.matches_with_ancestors(root, child_index, sibling_count, ancestors) {
                match rule.pseudo_element {
                    PseudoElement::Before => {
                        if let Some(content) = rule.declarations.get("content") {
                            style.before_content = resolve_content_value(content);
                        }
                    }
                    PseudoElement::After => {
                        if let Some(content) = rule.declarations.get("content") {
                            style.after_content = resolve_content_value(content);
                        }
                    }
                    PseudoElement::None => {
                        matched.push((rule.specificity, rule.declarations.clone()));
                    }
                }
                break;
            }
        }
    }
    matched.sort_by_key(|(sp, _)| *sp);
    for (_, decls) in &matched {
        for (prop, val) in decls {
            let resolved = resolve_var_references(val, &stylesheet.variables);
            apply_property(&mut style, prop, &resolved);
        }
    }

    // Apply inline style attribute
    if let Some(inline_style) = root.attributes.get("style").cloned() {
        let decls = parse_declarations(&inline_style);
        for (prop, val) in &decls {
            let resolved = resolve_var_references(val, &stylesheet.variables);
            apply_property(&mut style, prop, &resolved);
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

    root.style = style.clone();

    // Build this box's AncestorInfo for children
    let my_info = AncestorInfo {
        tag:           root.tag.clone(),
        attributes:    root.attributes.clone(),
        child_index,
        sibling_count,
    };
    let mut child_ancestors = ancestors.to_vec();
    child_ancestors.push(my_info);

    let n_children = root.children.len();
    for (i, child) in root.children.iter_mut().enumerate() {
        apply_cascade_inner(child, stylesheet, Some(&style), root_font_px, &child_ancestors, i, n_children);
    }
}

// ─── User-Agent Stylesheet ───────────────────────────────────────────────────

pub fn ua_stylesheet() -> Stylesheet {
    let mut ss = Stylesheet::default();
    ss.parse_and_add(UA_CSS);
    ss
}

const UA_CSS: &str = r##"
head, link, meta, script, style, title { display: none; }
html { display: block; }
body { display: block; margin: 0; }
article, aside, nav, section { display: block; }
h1 { display: block; font-size: 2em; font-weight: bold; margin-top: 0.67em; margin-bottom: 0.67em; break-after: avoid; break-inside: avoid; }
h2 { display: block; font-size: 1.5em; font-weight: bold; margin-top: 0.83em; margin-bottom: 0.83em; break-after: avoid; break-inside: avoid; }
h3 { display: block; font-size: 1.17em; font-weight: bold; margin-top: 1em; margin-bottom: 1em; break-after: avoid; break-inside: avoid; }
h4 { display: block; font-size: 1em; font-weight: bold; margin-top: 1.33em; margin-bottom: 1.33em; break-after: avoid; break-inside: avoid; }
h5 { display: block; font-size: 0.83em; font-weight: bold; margin-top: 1.67em; margin-bottom: 1.67em; break-after: avoid; break-inside: avoid; }
h6 { display: block; font-size: 0.67em; font-weight: bold; margin-top: 2.33em; margin-bottom: 2.33em; break-after: avoid; break-inside: avoid; }
hgroup { display: block; }
div, header, footer, main, search { display: block; }
p  { display: block; margin-top: 1em; margin-bottom: 1em; }
address { display: block; font-style: italic; }
blockquote { display: block; margin-top: 1em; margin-bottom: 1em; margin-left: 40px; margin-right: 40px; }
center { display: block; text-align: center; }
figure { display: block; margin-top: 1em; margin-bottom: 1em; margin-left: 40px; margin-right: 40px; }
figcaption { display: block; }
details { display: block; }
summary { display: list-item; list-style-type: disclosure-closed; }
pre { display: block; font-family: monospace; white-space: pre; margin-top: 1em; margin-bottom: 1em; }
hr  { display: block; margin-top: 0.5em; margin-bottom: 0.5em; border-style: inset; border-width: 1px; overflow: hidden; color: gray; }
dl, ol, ul { display: block; margin-top: 1em; margin-bottom: 1em; }
ol, ul { padding-left: 40px; }
dd, dt { display: block; }
dd { margin-left: 40px; }
li { display: list-item; }
ol { list-style-type: decimal; }
ul { list-style-type: disc; }
ul ul, ul ol, ol ul, ol ol { margin-top: 0; margin-bottom: 0; }
ul ul, ol ul { list-style-type: circle; }
ul ul ul, ul ol ul, ol ul ul, ol ol ul { list-style-type: square; }
cite, dfn, em, i, var { font-style: italic; }
b, strong { font-weight: bold; }
code, kbd, samp, tt { font-family: monospace; }
small { font-size: 0.83em; }
big  { font-size: 1.17em; }
sub  { vertical-align: sub; font-size: 0.83em; line-height: normal; }
sup  { vertical-align: super; font-size: 0.83em; line-height: normal; }
mark { background-color: yellow; color: black; }
a { color: #0000ee; text-decoration: underline; cursor: pointer; }
u, ins { text-decoration: underline; }
s, strike, del { text-decoration: line-through; }
abbr { text-decoration: underline dotted; }
q::before { content: open-quote; }
q::after  { content: close-quote; }
nobr { white-space: nowrap; }
br { display: inline; }
img, svg { display: inline-block; break-inside: avoid; }
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
  padding-left: 6px; padding-right: 6px; cursor: default;
}
input { display: inline-block; }
select { display: inline-block; }
textarea { display: inline-block; }
fieldset { display: block; margin-left: 2px; margin-right: 2px; padding-top: 0.35em; padding-bottom: 0.625em; padding-left: 0.75em; padding-right: 0.75em; border: 2px groove #ccc; }
legend { padding-left: 2px; padding-right: 2px; }
bdo { unicode-bidi: bidi-override; }
bdi { unicode-bidi: isolate; }
ruby { display: ruby; }
rt   { display: ruby-text; font-size: 0.5em; }
"##;
