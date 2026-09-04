//! The cascade: the public entry points AND the core walk they run.
//!
//! ⛔ `apply_cascade_inner` used to live in `cascade_incremental.rs`, which
//! left this file as three wrappers and put the actual cascade behind a name
//! that said "incremental". The incremental HOVER path is the other file's
//! subject; the walk is this one's.

#![allow(unused_imports)]
use super::*;
use crate::types::*;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

// ─── CSS Cascade ─────────────────────────────────────────────────────────────

fn normal_cascade_sort_key(rules: &[CssRule], specificity: u32, rule_idx: usize) -> (u8, u32, u32) {
    let origin_rank = if is_author_origin(specificity) { 1 } else { 0 };
    (origin_rank, rules[rule_idx].layer_rank, specificity)
}

fn important_cascade_sort_key(rules: &[CssRule], specificity: u32, rule_idx: usize) -> (u32, u32) {
    let layer_rank = rules[rule_idx].layer_rank;
    let reversed_layer_rank = if layer_rank == u32::MAX {
        0
    } else {
        u32::MAX - layer_rank
    };
    (reversed_layer_rank, specificity)
}

/// Apply a stylesheet to all boxes in the tree (cascade + inheritance).
pub fn apply_cascade(
    root: &mut crate::types::WebCore,
    stylesheet: &Stylesheet,
    parent_style: Option<&ComputedStyle>,
    root_font_px: f32,
) {
    apply_cascade_vp(
        root,
        stylesheet,
        parent_style,
        root_font_px,
        0.0,
        0.0,
        0,
        false,
    );
}

/// Apply a stylesheet with viewport size and focused element for media queries and :focus selectors.
///
/// `keyboard_focus` controls whether `:focus-visible` matches: pass `true` only when
/// focus was moved by keyboard (Tab/Shift+Tab), `false` for mouse-click focus.
///
/// **Note**: call `stylesheet.rebuild_index()` before this if rules were added since last cascade.
pub fn apply_cascade_vp(
    root: &mut crate::types::WebCore,
    stylesheet: &Stylesheet,
    parent_style: Option<&ComputedStyle>,
    root_font_px: f32,
    vw: f32,
    vh: f32,
    focused_box: u32,
    keyboard_focus: bool,
) {
    let empty_hover = std::collections::HashSet::new();
    apply_cascade_vp_hover(
        root,
        stylesheet,
        parent_style,
        root_font_px,
        vw,
        vh,
        focused_box,
        keyboard_focus,
        &empty_hover,
    );
}

/// Cascade with hover chain: elements in hover_chain will match :hover pseudo-class.
///
/// When the stylesheet has more than 1000 rules, automatically uses a parallel
/// selector-matching pass (via Rayon) to speed up large pages.
pub fn apply_cascade_vp_hover(
    root: &mut crate::types::WebCore,
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
        apply_cascade_parallel(
            root,
            stylesheet,
            parent_style,
            root_font_px,
            vw,
            vh,
            focused_box,
            keyboard_focus,
            hover_chain,
        );
        return;
    }
    // A single Vec is reused for the entire tree traversal (push/pop per node)
    // instead of cloning the ancestor list at every level — O(depth) allocations
    // instead of O(nodes × depth).
    let mut ancestors: Vec<AncestorInfo> = Vec::new();
    let mut candidates_buf: Vec<usize> = Vec::new();
    let mut counters: HashMap<String, Vec<i32>> = HashMap::new();
    let mut share_cache: ShareCache = HashMap::new();
    apply_cascade_inner(
        root,
        stylesheet,
        parent_style,
        root_font_px,
        &mut ancestors,
        0,
        1,
        0,
        1,
        vw,
        vh,
        focused_box,
        keyboard_focus,
        &stylesheet.variables,
        &mut candidates_buf,
        &mut counters,
        hover_chain,
        &[],
        &[],
        &mut share_cache,
        None,
    );
}

/// Build a ComputedStyle for a ::before/::after pseudo-element.
/// Inherits inherited properties from `base` (the originating element's style),
/// resets non-inherited properties to CSS initial values, then applies matched declarations.
/// The generated text of a `content` declaration, or `None` when it names no
/// pseudo-element at all. `content: ""` returns `Some("")`.
fn pseudo_content_value(value: &str) -> Option<String> {
    let v = value.trim();
    if v.eq_ignore_ascii_case("none") || v.eq_ignore_ascii_case("normal") {
        return None;
    }
    Some(value.to_string())
}

/// The last word on `display`, run once every declaration has been applied.
///
/// ⛔ COMPUTED-VALUE TIME, not declaration time (CSS Display 3 §2.7). A
/// blockification done inside `float`'s own applier depends on where `float`
/// sits among the declarations, so `float:left; display:inline` and
/// `display:inline; float:left` came out different — and the two cascade
/// implementations, which order matched rules differently, disagreed about the
/// same element on a re-cascade.
pub(crate) fn finalize_display(style: &mut ComputedStyle, tag: &str, has_explicit_display: bool) {
    crate::css::finalize_logical_float_clear(style);
    // A block-level element left Inline by nothing but the default takes Block.
    if matches!(style.display, Display::Inline) && !has_explicit_display {
        let should_be_block = matches!(
            tag,
            "div"
                | "p"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "ul"
                | "ol"
                | "dl"
                | "dt"
                | "dd"
                | "pre"
                | "blockquote"
                | "hr"
                | "section"
                | "article"
                | "aside"
                | "nav"
                | "header"
                | "footer"
                | "main"
                | "address"
                | "figure"
                | "figcaption"
                | "details"
                | "center"
                | "form"
                | "fieldset"
                | "legend"
                | "hgroup"
                | "search"
        );
        if should_be_block {
            style.display = Display::Block;
        }
    }
    // Floated and absolutely positioned boxes are blockified.
    let out_of_flow = !matches!(style.float, Float::None)
        || matches!(style.position, Position::Absolute | Position::Fixed);
    if out_of_flow {
        style.display = match style.display {
            Display::Inline | Display::InlineBlock => Display::Block,
            Display::InlineFlex => Display::Flex,
            Display::InlineGrid => Display::Grid,
            Display::TableRow
            | Display::TableCell
            | Display::TableHeaderCell
            | Display::TableRowGroup
            | Display::TableHeaderGroup
            | Display::TableFooterGroup
            | Display::TableColumn
            | Display::TableColumnGroup
            | Display::TableCaption
            | Display::Ruby
            | Display::RubyText => Display::Block,
            other => other,
        };
    }
}

fn blockify_flex_or_grid_item(style: &mut ComputedStyle) {
    style.display = match style.display {
        Display::Inline | Display::InlineBlock => Display::Block,
        Display::InlineFlex => Display::Flex,
        Display::InlineGrid => Display::Grid,
        Display::TableRow
        | Display::TableCell
        | Display::TableHeaderCell
        | Display::TableRowGroup
        | Display::TableHeaderGroup
        | Display::TableFooterGroup
        | Display::TableColumn
        | Display::TableColumnGroup
        | Display::TableCaption
        | Display::Ruby
        | Display::RubyText => Display::Block,
        other => other,
    };
}

pub(crate) fn build_pseudo_style_shared(
    matched: &mut Vec<(u32, usize)>,
    base: &ComputedStyle,
    vars: &HashMap<String, String>,
    attrs: &crate::dom::attrs::AttrMap,
    rules: &[CssRule],
) -> Option<(Option<String>, Box<ComputedStyle>)> {
    if matched.is_empty() {
        return None;
    }
    // CSS Cascade order for normal declarations is origin, then layer, then
    // specificity/source order. Keeping origin first prevents a UA unlayered
    // rule from beating a layered author rule.
    matched.sort_by_key(|(sp, idx)| normal_cascade_sort_key(rules, *sp, *idx));
    let mut ps = base.clone();
    // Reset non-inherited properties that should not leak from the originating element.
    ps.display = Display::Inline;
    ps.width = CssLength::Auto;
    ps.height = CssLength::Auto;
    ps.counter_reset.clear();
    ps.counter_increment.clear();
    ps.counter_set.clear();
    ps.before_style = None; // pseudo-elements don't nest
    ps.after_style = None;
    ps.placeholder_style = None;
    ps.before_content = String::new();
    ps.after_content = String::new();
    ps.marker_content = String::new();
    // **`content` decides whether the pseudo-element exists at all**
    // (css-pseudo-4 §2.1): `none` — which is what `normal` computes to here,
    // and what an absent declaration leaves — generates nothing. `""` is a
    // real, empty pseudo-element, so the two cannot collapse to one string.
    let mut content_value: Option<String> = None;
    for &(_, ri) in matched.iter() {
        for (prop, val) in &rules[ri].declarations {
            let resolved = resolve_var_references(val, vars);
            if prop == "content" {
                content_value = pseudo_content_value(&resolved);
            } else {
                apply_property(&mut ps, prop, &resolved);
            }
        }
    }
    // `!important` reverses the origin order (CSS Cascade §6.3): author first,
    // then UA, so a UA `!important` on a pseudo-element still wins.
    for author_pass in [true, false] {
        let mut important_matched = matched.clone();
        important_matched.sort_by_key(|(sp, idx)| important_cascade_sort_key(rules, *sp, *idx));
        for &(sp, ri) in important_matched.iter() {
            if is_author_origin(sp) != author_pass {
                continue;
            }
            for (prop, val) in &rules[ri].important_declarations {
                let resolved = resolve_var_references(val, vars);
                if prop == "content" {
                    content_value = pseudo_content_value(&resolved);
                } else {
                    apply_property(&mut ps, prop, &resolved);
                }
            }
        }
    }
    let content = content_value.as_deref().map(|value| {
        resolve_content_value_with_context(value, Some(attrs), Some(&ps.rare().quotes))
    });
    Some((content, Box::new(ps)))
}

/// Create or update the `::before` / `::after` child boxes.
///
/// ⛔ A FUNCTION, not the block it used to be. Its locals — two `WebCore`
/// pseudo-element boxes and their style clones — were living in
/// `apply_cascade_inner`'s frame ACROSS the recursive call, because a
/// debug build does not reuse stack slots between sibling scopes. Only a
/// real function boundary pops them (`arenaplan.md` item 3).
pub(crate) fn build_pseudo_element_boxes(root: &mut crate::types::WebCore) {
    let is_grid_or_flex = matches!(
        root.style.display,
        Display::Grid | Display::InlineGrid | Display::Flex | Display::InlineFlex
    );
    let before_is_positioned = root.style.before_style.as_ref().map_or(false, |ps| {
        matches!(ps.position, Position::Absolute | Position::Fixed)
    });
    let before_is_block = root
        .style
        .before_style
        .as_ref()
        .map_or(false, |ps| ps.is_block_level());
    // `before_style` is Some only when `content` generated the pseudo-element,
    // so it — not the generated TEXT, which is empty for `content: ""` — is
    // what says the box may exist.
    let before_generated = root.style.before_style.is_some();
    if before_generated && (is_grid_or_flex || before_is_positioned || before_is_block) {
        let existing = root.children.iter().position(|c| c.tag == "::before");
        let mut pseudo_box = crate::types::WebCore::new("::before");
        pseudo_box.text = root.style.before_content.clone();
        pseudo_box.tag = "::before".to_string();
        if let Some(ref ps) = root.style.before_style {
            pseudo_box.style = std::sync::Arc::new(*ps.clone());
        }
        if is_grid_or_flex
            && !pseudo_box.style.is_positioned()
            && matches!(pseudo_box.style.display, Display::Inline)
        {
            std::sync::Arc::make_mut(&mut pseudo_box.style).display = Display::Block;
        }
        if let Some(idx) = existing {
            root.children[idx] = pseudo_box;
        } else {
            root.children.insert(0, pseudo_box);
        }
        std::sync::Arc::make_mut(&mut root.style).before_content = String::new();
    } else {
        if let Some(idx) = root.children.iter().position(|c| c.tag == "::before") {
            root.children.remove(idx);
        }
    }
    let after_is_positioned = root.style.after_style.as_ref().map_or(false, |ps| {
        matches!(ps.position, Position::Absolute | Position::Fixed)
    });
    let after_is_block = root
        .style
        .after_style
        .as_ref()
        .map_or(false, |ps| ps.is_block_level());
    let after_generated = root.style.after_style.is_some();
    if after_generated && (is_grid_or_flex || after_is_positioned || after_is_block) {
        let existing = root.children.iter().position(|c| c.tag == "::after");
        let mut pseudo_box = crate::types::WebCore::new("::after");
        pseudo_box.text = root.style.after_content.clone();
        pseudo_box.tag = "::after".to_string();
        if let Some(ref ps) = root.style.after_style {
            pseudo_box.style = std::sync::Arc::new(*ps.clone());
        }
        if is_grid_or_flex
            && !pseudo_box.style.is_positioned()
            && matches!(pseudo_box.style.display, Display::Inline)
        {
            std::sync::Arc::make_mut(&mut pseudo_box.style).display = Display::Block;
        }
        if let Some(idx) = existing {
            root.children[idx] = pseudo_box;
        } else {
            root.children.push(pseudo_box);
        }
        std::sync::Arc::make_mut(&mut root.style).after_content = String::new();
    } else {
        if let Some(idx) = root.children.iter().position(|c| c.tag == "::after") {
            root.children.remove(idx);
        }
    }
}

/// A style-sharing cache for one cascade run, spanning the WHOLE document.
///
/// ⛔ The cache used to live inside `cascade_children`, so it only ever shared
/// between SIBLINGS — which is why the measured sharing on demo.html was 2.9%
/// while `arenaplan.md` quotes a 5-12x DOCUMENT-WIDE distinct-style ratio. The
/// two are different questions.
///
/// The key is `(parent style identity, tag, attributes)`. The parent's identity
/// is what makes a document-wide cache SOUND: two elements share only if their
/// parents already shared a style, which by induction means their whole
/// ancestor chains are selector-equivalent — so a descendant selector cannot
/// tell them apart. The base case is the root, whose style is unique.
///
/// Item 1 is what made this cheap: a parent style is an `Arc` now, so its
/// identity is a pointer rather than a deep comparison.
pub(crate) type ShareCache = HashMap<(usize, String, String), std::sync::Arc<ComputedStyle>>;

/// Every rule that matched one element, bucketed by what it styles.
///
/// ⛔ ONE shape for both cascades. The parallel pass computes these off-thread
/// and hands them to `apply_cascade_inner`, which otherwise computes them
/// itself — so there is a single matcher, a single set of buckets, and no way
/// for the two paths to disagree about which rules apply to an element.
#[derive(Clone, Default)]
pub(crate) struct MatchSets {
    pub matched: Vec<(u32, usize)>,
    pub hover_matched: Vec<(u32, usize)>,
    pub active_matched: Vec<(u32, usize)>,
    pub visited_matched: Vec<(u32, usize)>,
    pub before_matched: Vec<(u32, usize)>,
    pub after_matched: Vec<(u32, usize)>,
    pub selection_matched: Vec<(u32, usize)>,
    pub placeholder_matched: Vec<(u32, usize)>,
    pub marker_matched: Vec<(u32, usize)>,
}

/// Precomputed match results, keyed by `node_id`.
///
/// ⛔ `node_id`, not a path through `children`. A path is invalidated the moment
/// `build_pseudo_element_boxes` inserts a `::before` at index 0 during the apply
/// walk: every later sibling then reads its neighbour's rules, and the last one
/// reads none at all. `node_id` is stable across that insertion.
pub(crate) type MatchMap = HashMap<u32, MatchSets>;

/// Run the selectors of `stylesheet` against one element.
///
/// The only place a selector is tested during a cascade. `candidates_buf` is a
/// scratch Vec the caller owns so the walk allocates once, not once per node.
pub(crate) fn match_rules(
    node: &crate::types::WebCore,
    stylesheet: &Stylesheet,
    ancestors: &[AncestorInfo],
    child_index: usize,
    sibling_count: usize,
    type_child_index: usize,
    type_sibling_count: usize,
    vw: f32,
    vh: f32,
    focused_box: u32,
    keyboard_focus: bool,
    hover_chain: &std::collections::HashSet<u32>,
    prev_siblings: &[(String, String, String)],
    next_siblings: &[(String, String, String)],
    candidates_buf: &mut Vec<usize>,
) -> MatchSets {
    // ⛔ `html_box` is the element itself, always. `:has()`, `:empty`, `:focus`,
    // `:focus-within`, `:modal`, `:popover-open`, `:checked`, `:indeterminate`
    // and `:placeholder-shown` all read the BOX; without one they answer false
    // or fall back to the content attribute, which is a different page.
    let match_ctx = MatchContext {
        focused_box,
        keyboard_focus,
        type_child_index,
        type_sibling_count,
        html_box: Some(node),
        hover_chain,
        element_id: node.node_id,
        prev_siblings,
        next_siblings,
    };

    let mut sets = MatchSets::default();

    // The selector index narrows the candidates instead of scanning every rule.
    let id = node.attributes.get("id").map(|s| s.as_str());
    let class_attr = node
        .attributes
        .get("class")
        .map(|s| s.as_str())
        .unwrap_or("");
    let classes: Vec<&str> = class_attr.split_whitespace().collect();
    stylesheet.candidate_rules(&node.tag, id, &classes, candidates_buf);

    for &rule_idx in candidates_buf.iter() {
        let rule = &stylesheet.rules[rule_idx];
        // Rules whose @media condition does not match the viewport are not in
        // the cascade at all.
        if !rule.media_condition.is_empty() && !evaluate_media(&rule.media_condition, vw, vh) {
            continue;
        }
        // Container rules need layout context — a post-layout pass applies them.
        if !rule.container_condition.is_empty() {
            continue;
        }
        if !rule_matches_scope(
            rule,
            node,
            ancestors,
            child_index,
            sibling_count,
            &match_ctx,
        ) {
            continue;
        }
        for sel in &rule.selectors {
            // Per-selector state flags are precomputed; nothing is scanned here.
            let has_hover = sel.has_hover;
            let has_active = sel.has_active;
            let has_visited = sel.has_visited;

            if (has_hover || has_active || has_visited)
                && rule.pseudo_element == PseudoElement::None
            {
                if matches_selector_with_ancestors(
                    &sel.base_parts,
                    &node.tag,
                    &node.attributes,
                    child_index,
                    sibling_count,
                    ancestors,
                    &match_ctx,
                ) {
                    if has_hover {
                        sets.hover_matched.push((rule.specificity, rule_idx));
                    }
                    if has_active {
                        sets.active_matched.push((rule.specificity, rule_idx));
                    }
                    if has_visited {
                        sets.visited_matched.push((rule.specificity, rule_idx));
                    }
                    // With a hover chain live, the FULL selector is tested too:
                    // a `:hover` rule that matches now applies as a normal rule,
                    // so it can change layout (`display: block` on a menu).
                    if has_hover
                        && !hover_chain.is_empty()
                        && sel.matches_with_ancestors_ctx(
                            node,
                            child_index,
                            sibling_count,
                            ancestors,
                            &match_ctx,
                        )
                    {
                        sets.matched.push((rule.specificity, rule_idx));
                    }
                    break;
                }
                continue;
            }
            if sel.matches_with_ancestors_ctx(
                node,
                child_index,
                sibling_count,
                ancestors,
                &match_ctx,
            ) {
                match rule.pseudo_element {
                    PseudoElement::Before => sets.before_matched.push((rule.specificity, rule_idx)),
                    PseudoElement::After => sets.after_matched.push((rule.specificity, rule_idx)),
                    PseudoElement::Selection => {
                        sets.selection_matched.push((rule.specificity, rule_idx))
                    }
                    PseudoElement::Placeholder => {
                        sets.placeholder_matched.push((rule.specificity, rule_idx))
                    }
                    PseudoElement::Marker => sets.marker_matched.push((rule.specificity, rule_idx)),
                    PseudoElement::None => sets.matched.push((rule.specificity, rule_idx)),
                    PseudoElement::Ignored => {}
                }
                break;
            }
        }
    }
    sets
}

fn rule_matches_scope(
    rule: &CssRule,
    node: &WebCore,
    ancestors: &[AncestorInfo],
    child_index: usize,
    sibling_count: usize,
    match_ctx: &MatchContext<'_>,
) -> bool {
    let Some(scope_selector) = &rule.scope_selector else {
        return true;
    };
    if scope_selector.matches_with_ancestors_ctx(
        node,
        child_index,
        sibling_count,
        ancestors,
        match_ctx,
    ) {
        return true;
    }
    for (i, ancestor) in ancestors.iter().enumerate() {
        let ancestor_ctx = MatchContext {
            focused_box: match_ctx.focused_box,
            keyboard_focus: match_ctx.keyboard_focus,
            type_child_index: ancestor.type_child_index,
            type_sibling_count: ancestor.type_sibling_count,
            html_box: None,
            hover_chain: match_ctx.hover_chain,
            element_id: ancestor.node_id,
            prev_siblings: &[],
            next_siblings: &[],
        };
        if matches_selector_with_ancestors(
            &scope_selector.parts,
            &ancestor.tag,
            &ancestor.attributes,
            ancestor.child_index,
            ancestor.sibling_count,
            &ancestors[i + 1..],
            &ancestor_ctx,
        ) {
            return true;
        }
    }
    false
}

pub(crate) fn apply_cascade_inner(
    root: &mut crate::types::WebCore,
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
    prev_siblings: &[(String, String, String)],
    next_siblings: &[(String, String, String)],
    share_cache: &mut ShareCache,
    // Selector matches computed off-thread by the parallel pass, keyed by
    // `node_id`. `None`, or a miss, means match inline — never "no rules".
    precomputed: Option<&MatchMap>,
) {
    // Guard against stack overflow on deeply nested DOMs.
    if ancestors.len() >= MAX_CASCADE_DEPTH {
        // Just inherit from parent and stop — the page may render slightly wrong
        // at extreme depth, but won't crash.
        if let Some(p) = parent_style {
            std::sync::Arc::make_mut(&mut root.style).inherit_from(p);
        }
        return;
    }

    // Text and comment nodes are not elements — they inherit from their
    // parent but must never match CSS selectors (including `*`).
    if !root.is_element() {
        if let Some(p) = parent_style {
            std::sync::Arc::make_mut(&mut root.style).inherit_from(p);
        }
        return;
    }

    // Synthetic ::before/::after children already have their style set.
    // Skip the cascade for them — just recurse into their children (if any) and return.
    if root.tag == "::before" || root.tag == "::after" {
        // Still inherit inheritable properties from parent
        if let Some(p) = parent_style {
            let saved_display = root.style.display;
            std::sync::Arc::make_mut(&mut root.style).inherit_from(p);
            std::sync::Arc::make_mut(&mut root.style).display = saved_display; // preserve blockified display
        }
        return;
    }

    // ⚠ A style-sharing stub stood here: four bindings feeding an EMPTY `if`,
    // whose own comment said the sharing "actually happens in cascade_children
    // where we have access to the sibling WebCore objects". It computed a class
    // string and a hover lookup on every element and did nothing with them.
    // Removed rather than annotated — a reader greps `class_attr` and lands on
    // machinery that never ran.

    // Start with default style and inherit from parent
    let mut style = ComputedStyle::default();
    if let Some(p) = parent_style {
        style.inherit_from(p);
        style.relative_font_weight_base = Some(p.font_weight);
    }

    // Apply presentational HTML attributes (specificity 0 — before author rules)
    let attrs = root.attributes.clone();
    for (attr, val) in &attrs {
        match attr.as_str() {
            "align" => match val.as_str() {
                "center" => apply_property(&mut style, "text-align", "center"),
                "right" => apply_property(&mut style, "text-align", "right"),
                "left" => apply_property(&mut style, "text-align", "left"),
                _ => {}
            },
            "valign" => apply_property(&mut style, "vertical-align", val),
            // `<select multiple>` with no `size` shows FOUR rows, which is the
            // long-standing UA default every browser uses. With a `size` the
            // arm below wins, because it is applied from that attribute.
            "multiple" if root.tag == "select" && !root.attributes.contains_key("size") => {
                apply_property(&mut style, "height", &format!("{}em", 4.0 * 1.2 + 0.5));
            }
            "bgcolor" => apply_property(&mut style, "background-color", val),
            "color" | "text" => apply_property(&mut style, "color", val),
            "face" => apply_property(&mut style, "font-family", val),
            // ⛔ `size` means THREE different things depending on the element,
            // and this arm applied the `<font>` one to all of them: a
            // `<select size="4">` — four visible ROWS — was being given
            // `font-size: 18px`, because `"4"` is also a legal `<font size>`.
            "size" => match root.tag.as_str() {
                // HTML <font size="1..7"> maps to absolute px sizes.
                "font" => {
                    let px: f32 = match val.trim() {
                        "1" => 10.0,
                        "2" => 13.0,
                        "3" => 16.0,
                        "4" => 18.0,
                        "5" => 24.0,
                        "6" => 32.0,
                        "7" => 48.0,
                        v => v.parse::<f32>().unwrap_or(16.0),
                    };
                    apply_property(&mut style, "font-size", &format!("{}px", px));
                }
                // **`<select size=N>` with N > 1 is a LIST BOX** (HTML
                // §4.10.7): it shows N options at once instead of one closed
                // row. The UA sheet's `height: 2.2em` is the closed height, so
                // the list needs its own — one line box per row plus the
                // border-box padding, which is what a browser computes.
                //
                // Presentational, so an author's own `height` still wins: this
                // is the UA's default for the attribute, not an override.
                "select" => {
                    // Every `<select size=N>` takes its height from here, N=1
                    // included: the UA rule deliberately does not match an
                    // element that HAS the attribute, so if this arm skipped
                    // `size="1"` that select would be left with no height at
                    // all. One row is the closed height.
                    let rows = val.trim().parse::<f32>().unwrap_or(1.0).max(1.0);
                    let height = if rows > 1.0 { rows * 1.2 + 0.5 } else { 2.2 };
                    apply_property(&mut style, "height", &format!("{height}em"));
                }
                // `<input size=N>` is a width in CHARACTERS — `ch` is exactly
                // that unit, and the UA sheet's fixed `width: 200px` is what it
                // replaces.
                "input" => {
                    if let Ok(chars) = val.trim().parse::<f32>() {
                        if chars > 0.0 {
                            apply_property(&mut style, "width", &format!("{}ch", chars));
                        }
                    }
                }
                _ => {}
            },
            "rows" if root.tag == "textarea" => {
                if let Ok(rows) = val.trim().parse::<f32>() {
                    if rows > 0.0 {
                        apply_property(&mut style, "height", &format!("{}em", rows * 1.4));
                    }
                }
            }
            "cols" if root.tag == "textarea" => {
                if let Ok(cols) = val.trim().parse::<f32>() {
                    if cols > 0.0 {
                        apply_property(&mut style, "width", &format!("{}em", cols * 0.6));
                    }
                }
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
                "ltr" => apply_property(&mut style, "direction", "ltr"),
                "auto" => {
                    let text = collect_text_for_dir_auto(root);
                    if let Some(dir) = crate::layout::text::first_strong_direction(&text) {
                        match dir {
                            Direction::RTL => apply_property(&mut style, "direction", "rtl"),
                            Direction::LTR => apply_property(&mut style, "direction", "ltr"),
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    // HTML: td/th inside a table with border="N" (N>0) get a 1px inset border,
    // matching browser UA behaviour. Applied at presentational-attribute specificity
    // so author CSS can override.
    if matches!(root.tag.as_str(), "td" | "th") {
        let has_table_border = ancestors.iter().rev().any(|a| {
            a.tag == "table"
                && a.attributes
                    .get("border")
                    .and_then(|v| v.parse::<f32>().ok())
                    .map_or(false, |n| n > 0.0)
        });
        if has_table_border {
            apply_property(&mut style, "border", "1px solid");
        }
    }

    // Selector matching — the SAME function the parallel pass runs, so a
    // precomputed result and an inline one can never disagree.
    let precomputed_here = precomputed
        .filter(|_| root.node_id != 0)
        .and_then(|m| m.get(&root.node_id));
    let sets = match precomputed_here {
        Some(sets) => sets.clone(),
        // ⛔ A miss MATCHES, it does not mean "no rules". An element the parallel
        // pass never saw — a box with no DOM node behind it, or a shadow subtree,
        // which is matched against its own scoped sheet — has to be cascaded, and
        // handing it an empty result renders it unstyled with nothing to show for it.
        None => match_rules(
            root,
            stylesheet,
            ancestors,
            child_index,
            sibling_count,
            type_child_index,
            type_sibling_count,
            vw,
            vh,
            focused_box,
            keyboard_focus,
            hover_chain,
            prev_siblings,
            next_siblings,
            candidates_buf,
        ),
    };
    let MatchSets {
        mut matched,
        mut hover_matched,
        mut active_matched,
        mut visited_matched,
        mut before_matched,
        mut after_matched,
        mut selection_matched,
        mut placeholder_matched,
        mut marker_matched,
    } = sets;
    matched.sort_by_key(|(sp, idx)| normal_cascade_sort_key(&stylesheet.rules, *sp, *idx));
    // Build variable scope: inherited from parent + any --custom-properties from matched rules.
    // Only clone the map when new custom properties are actually defined — most elements
    // don't define any, so we avoid O(vars) cloning at every node.
    let has_new_vars = matched.iter().any(|(_, ri)| {
        stylesheet.rules[*ri]
            .declarations
            .keys()
            .any(|p| p.starts_with("--"))
            || stylesheet.rules[*ri]
                .important_declarations
                .keys()
                .any(|p| p.starts_with("--"))
    });
    // Also check inline style for custom properties — these must be available
    // during var() resolution of stylesheet rules on the same element.
    let inline_decls = root
        .attributes
        .get("style")
        .cloned()
        .map(|s| parse_declarations_important(&s));
    let has_inline_vars = inline_decls
        .as_ref()
        .map(|(n, _)| n.keys().any(|p| p.starts_with("--")))
        .unwrap_or(false);

    let local_vars_owned = if has_new_vars || has_inline_vars {
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
        Some(vars)
    } else {
        None
    };
    let local_vars: &HashMap<String, String> = local_vars_owned.as_ref().unwrap_or(inherited_vars);
    // Track properties whose highest-specificity declaration is `inherit`.
    // After all rules are applied, these properties are reset to the parent's value.
    let mut inherit_props: HashSet<String> = HashSet::new();
    let has_vars = !local_vars.is_empty();
    let mut pre_author_normal_style: Option<ComputedStyle> = None;
    for &(sp, ri) in &matched {
        if is_author_origin(sp) && pre_author_normal_style.is_none() {
            pre_author_normal_style = Some(style.clone());
        }
        let revert_base = if is_author_origin(sp) {
            pre_author_normal_style.as_ref()
        } else {
            None
        };
        // Fast path: use pre-compiled declarations (PropertyId dispatch, no string matching).
        // Only fall back to raw declarations when var() resolution is needed.
        let rule = &stylesheet.rules[ri];
        if has_vars && rule.has_var_refs {
            // Slow path: var() references need string-based resolution
            for (prop, val) in &rule.declarations {
                if prop.starts_with("--") {
                    continue;
                }
                let resolved = resolve_var_references(val, &local_vars);
                if resolved.trim().is_empty() && val.contains("var(") {
                    continue;
                }
                let trimmed = resolved.trim();
                if trimmed == "inherit" {
                    inherit_props.insert(prop.to_string());
                } else if matches!(trimmed, "revert" | "revert-layer") {
                    if let Some(base) = revert_base {
                        copy_property_from_style(&mut style, base, prop);
                    } else {
                        apply_property(&mut style, prop, "initial");
                    }
                } else {
                    inherit_props.remove(prop.as_str());
                    apply_property(&mut style, prop, &resolved);
                }
            }
        } else {
            // Fast path: no var() — use compiled declarations directly
            for &(id, ref val) in &rule.compiled_decls {
                if matches!(val, crate::types::CssValue::Inherit) {
                    let name = property_defs::get(id).name;
                    inherit_props.insert(name.to_string());
                } else if matches!(
                    val,
                    crate::types::CssValue::Revert | crate::types::CssValue::RevertLayer
                ) {
                    let name = property_defs::get(id).name;
                    if let Some(base) = revert_base {
                        copy_property_from_style(&mut style, base, name);
                    } else {
                        apply_css_value(&mut style, id, &crate::types::CssValue::Initial);
                    }
                } else if let crate::types::CssValue::Raw(s) = val {
                    // Raw values may contain var() even when has_vars is false
                    // (the rule has var refs but no variables are defined in scope).
                    // Resolve var() with empty vars — triggers fallback values.
                    if s.contains("var(") {
                        let resolved = resolve_var_references(s, &local_vars);
                        if !resolved.trim().is_empty() {
                            let trimmed = resolved.trim();
                            if matches!(trimmed, "revert" | "revert-layer") {
                                if let Some(base) = revert_base {
                                    copy_property_from_style(
                                        &mut style,
                                        base,
                                        property_defs::get(id).name,
                                    );
                                } else {
                                    apply_css_value(
                                        &mut style,
                                        id,
                                        &crate::types::CssValue::Initial,
                                    );
                                }
                            } else {
                                apply_property_by_id_str(&mut style, id, &resolved);
                            }
                        }
                    } else {
                        apply_css_value(&mut style, id, val);
                    }
                } else {
                    apply_css_value(&mut style, id, val);
                }
            }
        }
    }

    apply_form_sizing_hints_after_ua(&mut style, root, &stylesheet.rules, &matched);

    // Second pass: `!important`, in CSS Cascade §6.3 order — which REVERSES the
    // origin ranking. A UA `!important` beats an author `!important`, so the UA
    // rules are applied LAST here even though they were applied first above.
    // `matched` is sorted by boosted specificity, so filtering on origin keeps
    // specificity order within each pass.
    //
    // Applying `matched` in one sweep let a page write
    // `input[type=hidden] { display: block !important }` and reveal a hidden
    // field — Chrome answers `display: none` there, and now so does this.
    let mut important_matched = matched.clone();
    important_matched
        .sort_by_key(|(sp, idx)| important_cascade_sort_key(&stylesheet.rules, *sp, *idx));
    for author_pass in [true, false] {
        for &(sp, ri) in &important_matched {
            if is_author_origin(sp) != author_pass {
                continue;
            }
            let rule = &stylesheet.rules[ri];
            if has_vars && rule.has_var_refs {
                for (prop, val) in &rule.important_declarations {
                    if prop.starts_with("--") {
                        continue;
                    }
                    let resolved = resolve_var_references(val, &local_vars);
                    if resolved.trim().is_empty() && val.contains("var(") {
                        continue;
                    }
                    apply_property(&mut style, prop, &resolved);
                }
            } else {
                for &(id, ref val) in &rule.compiled_important {
                    apply_css_value(&mut style, id, val);
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
        hover_matched
            .sort_by_key(|(sp, idx)| normal_cascade_sort_key(&stylesheet.rules, *sp, *idx));
        let mut hs = style.clone();
        for &(_, ri) in &hover_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_decls {
                apply_css_value_with_vars(&mut hs, id, val, &local_vars);
            }
        }
        for &(_, ri) in &hover_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
                apply_css_value_with_vars(&mut hs, id, val, &local_vars);
            }
        }
        // Prevent infinite nesting: state styles don't carry their own state overrides.
        hs.hover_style = None;
        hs.active_style = None;
        hs.visited_style = None;
        style.hover_style = Some(Box::new(hs));
    }
    // Active style — clone the base style and overlay all matched active declarations.
    if !active_matched.is_empty() {
        active_matched
            .sort_by_key(|(sp, idx)| normal_cascade_sort_key(&stylesheet.rules, *sp, *idx));
        let mut as_ = style.clone();
        for &(_, ri) in &active_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_decls {
                apply_css_value_with_vars(&mut as_, id, val, &local_vars);
            }
        }
        for &(_, ri) in &active_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
                apply_css_value_with_vars(&mut as_, id, val, &local_vars);
            }
        }
        as_.hover_style = None;
        as_.active_style = None;
        as_.visited_style = None;
        style.active_style = Some(Box::new(as_));
    }
    // Visited style — clone the base style and overlay all matched visited declarations.
    if !visited_matched.is_empty() {
        visited_matched
            .sort_by_key(|(sp, idx)| normal_cascade_sort_key(&stylesheet.rules, *sp, *idx));
        let mut vs = style.clone();
        for &(_, ri) in &visited_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_decls {
                apply_css_value_with_vars(&mut vs, id, val, &local_vars);
            }
        }
        for &(_, ri) in &visited_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
                apply_css_value_with_vars(&mut vs, id, val, &local_vars);
            }
        }
        vs.hover_style = None;
        vs.active_style = None;
        vs.visited_style = None;
        style.visited_style = Some(Box::new(vs));
    }

    // Apply inline style attribute (normal declarations).
    // Custom properties were already merged into local_vars above.
    // Also collect inline hover-* properties for building hover_style.
    let mut inline_hover_props: Vec<(String, String)> = Vec::new();
    let (_inline_normal, inline_important) = if let Some((n, i)) = inline_decls {
        for (prop, val) in &n {
            if prop.starts_with("--") {
                continue;
            }
            // Inline hover-* properties: hover-background-color → background-color on hover
            if let Some(real_prop) = prop.strip_prefix("hover-") {
                let resolved = resolve_var_references(val, local_vars);
                inline_hover_props.push((real_prop.to_string(), resolved));
                continue;
            }
            let resolved = resolve_var_references(val, local_vars);
            if resolved.trim() == "inherit" {
                if let Some(p) = parent_style {
                    copy_property_from_parent(&mut style, p, prop);
                }
            } else if matches!(resolved.trim(), "revert" | "revert-layer") {
                if let Some(base) = &pre_author_normal_style {
                    copy_property_from_style(&mut style, base, prop);
                } else {
                    apply_property(&mut style, prop, "initial");
                }
            } else {
                apply_property(&mut style, prop, &resolved);
            }
        }
        (n, i)
    } else {
        (Declarations::new(), Declarations::new())
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
        hs.hover_style = None;
        hs.active_style = None;
        hs.visited_style = None;
        style.hover_style = Some(Box::new(hs));
    }

    // `!important`, in CSS Cascade §6.4.1 order — bottom to top:
    //   author sheet important → inline important → UA important.
    // The style attribute is author origin, so it outranks author RULES but
    // still loses to the UA sheet's `!important`; the UA pass therefore comes
    // last, not first.
    for &(sp, ri) in &important_matched {
        if !is_author_origin(sp) {
            continue;
        }
        for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
            apply_css_value_with_vars(&mut style, id, val, &local_vars);
        }
    }
    for (prop, val) in &inline_important {
        let resolved = resolve_var_references(val, &local_vars);
        if resolved.trim() == "inherit" {
            if let Some(p) = parent_style {
                copy_property_from_parent(&mut style, p, prop);
            }
        } else {
            apply_property(&mut style, prop, &resolved);
        }
    }
    for &(sp, ri) in &important_matched {
        if is_author_origin(sp) {
            continue;
        }
        for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
            apply_css_value_with_vars(&mut style, id, val, &local_vars);
        }
    }

    // Re-apply table layout HTML attributes after CSS rules so UA/author stylesheets
    // cannot silently override them (e.g. UA "border-spacing: 2px" must not win over
    // cellspacing="0").  These are still below inline style priority.
    //
    // `valign` joins them for the same reason: the UA sheet gives `td`/`th` the
    // default `vertical-align: middle` a cell must have, and a presentational
    // hint has to outrank the UA sheet (HTML §15.2 places hints at the start of
    // the AUTHOR origin). The general ordering — hints applied before every
    // matched rule instead of between the UA and author passes — is still wrong
    // for `align`, `bgcolor`, `width` and `height`; see cssgaps.md.
    if matches!(
        root.tag.as_str(),
        "td" | "th" | "tr" | "thead" | "tbody" | "tfoot"
    ) {
        if let Some(v) = root.attributes.get("valign").cloned() {
            apply_property(&mut style, "vertical-align", &v);
        }
    }
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
    let parent_font_px = parent_style
        .map(|p| p.font_size_px(root_font_px, root_font_px))
        .unwrap_or(root_font_px);
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
    let has_explicit_display = matched.iter().any(|&(_, ri)| {
        stylesheet.rules[ri]
            .declarations
            .iter()
            .any(|(k, _)| k == "display")
    });
    finalize_display(&mut style, &root.tag, has_explicit_display);
    // `currentColor` resolves against this element's own `color`, which is only
    // final now — css-color-4 §6.2.
    crate::css::finalize_current_color(&mut style);
    // Flow-relative box properties map onto physical sides using the FINAL
    // `direction` and `writing-mode` — css-logical-1 §4.
    crate::css::finalize_logical(&mut style);
    // ⛔ MOVED, not cloned. This was a full 2.3 KB `ComputedStyle` copy per
    // element — and it kept the local alive across the recursive call below,
    // where it is the single biggest thing in the stack frame.
    //
    // ⛔ NOT interned. Giving byte-identical styles the same `Arc` was built
    // and MEASURED: it took demo.html from 1,099 distinct styles to 979 (11%)
    // and more than doubled cascade+layout time, 2.46 s to 5.80 s. The
    // `arenaplan.md` 5-12x figure is a measurement artifact — that plan's own
    // footnote says the serializer it counted with "emits a subset of
    // properties, so styles differing in an unserialized property collide".
    // Compared losslessly the styles on a real page are nearly all distinct.
    root.style = std::sync::Arc::new(style);
    // Store matched CSS rules for inspector (only when enabled).
    if stylesheet.inspect_mode {
        root.matched_rules.clear();
        for &(sp, ri) in &matched {
            let rule = &stylesheet.rules[ri];
            root.matched_rules.push(crate::types::MatchedRule {
                selector: rule.original_selector.clone(),
                declarations: rule
                    .declarations
                    .iter()
                    .map(|(k, v)| (k.clone(), resolve_var_references(v, &local_vars)))
                    .collect(),
                specificity: sp,
                source: if ri < 50 {
                    "ua".to_string()
                } else {
                    rule.media_condition.clone()
                },
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
        let in_table = ancestors
            .iter()
            .any(|a| matches!(a.tag.as_str(), "table" | "thead" | "tbody" | "tfoot" | "tr"));
        if in_table {
            std::sync::Arc::make_mut(&mut root.style).display = Display::Contents;
        }
    }

    // Build full ComputedStyle for ::before / ::after pseudo-elements.
    // Each inherits from the element's computed style, then has its own declarations applied.
    // ── CSS counters: reset, increment, then resolve counter() in content ──
    // Track which counters were reset at this level so we can pop them later.
    let mut counters_pushed: Vec<String> = Vec::new();
    fn increment_counter(
        counters: &mut HashMap<String, Vec<i32>>,
        counters_pushed: &mut Vec<String>,
        name: &str,
        delta: i32,
    ) {
        let stack = counters.entry(name.to_string()).or_insert_with(|| {
            counters_pushed.push(name.to_string());
            vec![0]
        });
        if let Some(top) = stack.last_mut() {
            *top += delta;
        }
    }
    fn set_counter(
        counters: &mut HashMap<String, Vec<i32>>,
        counters_pushed: &mut Vec<String>,
        name: &str,
        value: i32,
    ) {
        let stack = counters.entry(name.to_string()).or_insert_with(|| {
            counters_pushed.push(name.to_string());
            vec![0]
        });
        if let Some(top) = stack.last_mut() {
            *top = value;
        }
    }
    for (name, val) in &root.style.counter_reset {
        counters
            .entry(name.clone())
            .or_insert_with(Vec::new)
            .push(*val);
        counters_pushed.push(name.clone());
    }
    // `ol` implicitly resets the `list-item` counter
    if root.tag == "ol" && root.style.counter_reset.is_empty() {
        counters
            .entry("list-item".to_string())
            .or_insert_with(Vec::new)
            .push(0);
        counters_pushed.push("list-item".to_string());
    }
    for (name, val) in &root.style.counter_increment {
        increment_counter(counters, &mut counters_pushed, name, *val);
    }
    for (name, val) in &root.style.counter_set {
        set_counter(counters, &mut counters_pushed, name, *val);
    }
    // `li` implicitly increments the `list-item` counter
    if root.tag == "li" && root.style.counter_increment.is_empty() {
        increment_counter(counters, &mut counters_pushed, "list-item", 1);
    }
    if root.tag == "li" {
        if let Some(value) = counters
            .get("list-item")
            .and_then(|stack| stack.last())
            .copied()
        {
            std::sync::Arc::make_mut(&mut root.style).list_index = value;
        }
    }

    if let Some((Some(txt), ps)) = build_pseudo_style_shared(
        &mut before_matched,
        &root.style,
        &local_vars,
        &root.attributes,
        &stylesheet.rules,
    ) {
        // ::before may carry counter-increment/counter-reset — apply before resolving content
        for (name, val) in &ps.counter_reset {
            counters
                .entry(name.clone())
                .or_insert_with(Vec::new)
                .push(*val);
            counters_pushed.push(name.clone());
        }
        for (name, val) in &ps.counter_increment {
            increment_counter(counters, &mut counters_pushed, name, *val);
        }
        for (name, val) in &ps.counter_set {
            set_counter(counters, &mut counters_pushed, name, *val);
        }
        std::sync::Arc::make_mut(&mut root.style).before_content =
            resolve_counters_in_content(&txt, counters);
        std::sync::Arc::make_mut(&mut root.style).before_style = Some(ps);
    }
    if let Some((Some(txt), ps)) = build_pseudo_style_shared(
        &mut after_matched,
        &root.style,
        &local_vars,
        &root.attributes,
        &stylesheet.rules,
    ) {
        for (name, val) in &ps.counter_reset {
            counters
                .entry(name.clone())
                .or_insert_with(Vec::new)
                .push(*val);
            counters_pushed.push(name.clone());
        }
        for (name, val) in &ps.counter_increment {
            increment_counter(counters, &mut counters_pushed, name, *val);
        }
        for (name, val) in &ps.counter_set {
            set_counter(counters, &mut counters_pushed, name, *val);
        }
        std::sync::Arc::make_mut(&mut root.style).after_content =
            resolve_counters_in_content(&txt, counters);
        std::sync::Arc::make_mut(&mut root.style).after_style = Some(ps);
    }
    if let Some((_, ps)) = build_pseudo_style_shared(
        &mut selection_matched,
        &root.style,
        &local_vars,
        &root.attributes,
        &stylesheet.rules,
    ) {
        std::sync::Arc::make_mut(&mut root.style).selection_style = Some(ps);
    }
    if let Some((_, ps)) = build_pseudo_style_shared(
        &mut placeholder_matched,
        &root.style,
        &local_vars,
        &root.attributes,
        &stylesheet.rules,
    ) {
        std::sync::Arc::make_mut(&mut root.style).placeholder_style = Some(ps);
    }
    if let Some((txt, ps)) = build_pseudo_style_shared(
        &mut marker_matched,
        &root.style,
        &local_vars,
        &root.attributes,
        &stylesheet.rules,
    ) {
        if let Some(txt) = txt {
            std::sync::Arc::make_mut(&mut root.style).marker_content =
                resolve_counters_in_content(&txt, counters);
        }
        std::sync::Arc::make_mut(&mut root.style).marker_style = Some(ps);
    }

    build_pseudo_element_boxes(root);

    ancestors.push(AncestorInfo {
        tag: root.tag.clone(),
        attributes: root.attributes.clone(),
        child_index,
        sibling_count,
        type_child_index,
        type_sibling_count,
        node_id: root.node_id,
    });

    // Helper: cascade a list of children with a given stylesheet
    fn cascade_children(
        children: &mut [crate::types::WebCore],
        stylesheet: &Stylesheet,
        // ⛔ The `Arc`, not a `&ComputedStyle`: the share key needs the
        // parent's IDENTITY, and that is the pointer.
        parent_style: &std::sync::Arc<ComputedStyle>,
        root_font_px: f32,
        ancestors: &mut Vec<AncestorInfo>,
        vw: f32,
        vh: f32,
        focused_box: u32,
        keyboard_focus: bool,
        inherited_vars: &HashMap<String, String>,
        candidates_buf: &mut Vec<usize>,
        counters: &mut HashMap<String, Vec<i32>>,
        hover_chain: &std::collections::HashSet<u32>,
        share_cache: &mut ShareCache,
        precomputed: Option<&MatchMap>,
    ) {
        let n_children = children.len();
        if n_children == 0 {
            return;
        }
        let child_tags: Vec<String> = children
            .iter()
            .map(|c| c.tag.to_ascii_lowercase())
            .collect();
        let mut type_running: HashMap<&str, usize> = HashMap::new();
        let type_counts: Vec<usize> = child_tags
            .iter()
            .map(|tag| {
                let slot = type_running.entry(tag.as_str()).or_insert(0);
                let idx = *slot;
                *slot += 1;
                idx
            })
            .collect();
        let type_totals: Vec<usize> = child_tags
            .iter()
            .map(|tag| *type_running.get(tag.as_str()).unwrap_or(&0))
            .collect();
        let n_elem_children = children.iter().filter(|c| c.is_element()).count();
        let mut elem_pos = 0usize;
        let elem_indices: Vec<usize> = children
            .iter()
            .map(|c| {
                if !c.is_element() {
                    0
                } else {
                    let p = elem_pos;
                    elem_pos += 1;
                    p
                }
            })
            .collect();
        // One sibling row feeds both left-looking combinators and
        // right-looking `:nth-last-child(... of S)`.
        let sibling_records: Vec<(String, String, String)> = children
            .iter()
            .filter(|c| c.is_element())
            .map(|c| {
                (
                    c.tag.clone(),
                    c.attributes.get("id").cloned().unwrap_or_default(),
                    c.attributes.get("class").cloned().unwrap_or_default(),
                )
            })
            .collect();
        // ⛔ The cache is the caller's now, spanning the whole document —
        // see `ShareCache`. A per-parent one could only ever share between
        // siblings, which measured 2.9% on demo.html.
        let parent_id = std::sync::Arc::as_ptr(parent_style) as usize;
        for (i, child) in children.iter_mut().enumerate() {
            let (ci, ns) = if !child.is_element() {
                (i, n_children)
            } else {
                (elem_indices[i], n_elem_children)
            };
            let sibling_pos = elem_indices[i];
            let (prev_for_child, next_for_child) = if child.is_element() {
                (
                    &sibling_records[..sibling_pos],
                    &sibling_records[sibling_pos.saturating_add(1)..],
                )
            } else {
                (&[][..], &[][..])
            };

            // Try style sharing before full cascade.
            //
            // ⛔ The key is the element's WHOLE attribute list, not just its
            // class. `i[data-x] { … }` was silently dropped: the two `<i>`
            // hashed the same and the second took the first one's style.
            // Confirmed to be sharing, not missing attribute-selector support,
            // by running the fixture with `can_share` forced false.
            //
            // The attributes ARE the element as far as a selector is concerned
            // — anything else is a hole waiting for the next selector form.
            // ⛔ …and the element's BOX STATE, which no attribute records.
            // `:modal`, `:checked`, `:focus`, `:indeterminate` and `:in-range`
            // all match on it, so two elements with the same tag and the same
            // attributes are still not interchangeable. Two `<dialog open>`s,
            // one `show()`n and one `showModal()`ed, hashed the same and the
            // modal took the plain one's `position`.
            let child_class = {
                let mut parts: Vec<String> = child
                    .attributes
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect();
                parts.sort();
                parts.push(child.selector_state_key(focused_box));
                parts.join("\u{1}")
            };
            let can_share = child.is_element()
                && child.tag != "::before" && child.tag != "::after"
                && !child.attributes.contains_key("id")
                && !child.attributes.contains_key("style")
                && !hover_chain.contains(&child.node_id)
                // ⛔ The share key is `(tag, class)` and says nothing about
                // sibling POSITION. With `i + i { … }` or `li:nth-child(2)`
                // in the sheet, two same-key siblings are NOT interchangeable
                // — the second was handed the first one's style and the rule
                // vanished. Verified both ways: the test goes green with
                // sharing off and red with it on.
                && !stylesheet.has_sibling_sensitive_rules
                && child.children.is_empty(); // only for leaf elements (no pseudo-elements to worry about)
            let share_key = (parent_id, child.tag.clone(), child_class.clone());

            if can_share {
                if let Some(cached) = share_cache.get(&share_key) {
                    // ⛔ THE point of item 1: a shared style is a refcount
                    // bump, not a 2.3 KB memcpy. `cached` is already an `Arc`.
                    child.style = cached.clone();
                    continue;
                }
            }

            apply_cascade_inner(
                child,
                stylesheet,
                Some(parent_style),
                root_font_px,
                ancestors,
                ci,
                ns,
                type_counts[i],
                type_totals[i],
                vw,
                vh,
                focused_box,
                keyboard_focus,
                inherited_vars,
                candidates_buf,
                counters,
                hover_chain,
                prev_for_child,
                next_for_child,
                share_cache,
                precomputed,
            );
            if matches!(
                parent_style.display,
                Display::Flex | Display::InlineFlex | Display::Grid | Display::InlineGrid
            ) {
                blockify_flex_or_grid_item(std::sync::Arc::make_mut(&mut child.style));
            }
            // Cache style for sharing with future siblings
            if can_share && !share_cache.contains_key(&share_key) {
                share_cache.insert(share_key, child.style.clone());
            }
        }
    }

    // Shadow DOM: cascade shadow children with the shadow's scoped stylesheet,
    // and also cascade light DOM children with the document stylesheet.
    // CSS custom properties cross the shadow boundary via inherited_vars.
    // ⛔ The parent style for the children comes from the ARC now, not from a
    // 2.3 KB local held alive across the recursion. This is the frame shrink:
    // what crosses the recursive call is a pointer.
    let parent_for_children = root.style.clone();
    if root.shadow_root.is_some() {
        // Take shadow root temporarily to satisfy borrow checker
        let mut sr = root.shadow_root.take().unwrap();
        sr.stylesheet.rebuild_index();
        // `:host` — the shadow stylesheet styling its own host. The matcher
        // cannot answer it (it has no idea whose shadow tree a rule came from),
        // so it is applied HERE, where the host and its shadow stylesheet are
        // both in hand. `:host` rules were returning false unconditionally,
        // which made the single most common shadow-CSS rule inert.
        apply_host_rules(root, &sr.stylesheet, &local_vars, ancestors);
        // ⛔ `None`, not the map: pass 1 walks the LIGHT tree only, and the map
        // is keyed globally by `node_id`. A shadow child that happened to carry
        // an id from the light pass would be handed rules matched against the
        // DOCUMENT sheet instead of its own scoped one.
        cascade_children(
            &mut sr.children,
            &sr.stylesheet,
            &parent_for_children,
            root_font_px,
            ancestors,
            vw,
            vh,
            focused_box,
            keyboard_focus,
            &local_vars,
            candidates_buf,
            counters,
            hover_chain,
            share_cache,
            None,
        );
        root.shadow_root = Some(sr);
        // Also cascade light DOM children (they need document styles for ::slotted)
        cascade_children(
            &mut root.children,
            stylesheet,
            &parent_for_children,
            root_font_px,
            ancestors,
            vw,
            vh,
            focused_box,
            keyboard_focus,
            &local_vars,
            candidates_buf,
            counters,
            hover_chain,
            share_cache,
            precomputed,
        );
    } else {
        cascade_children(
            &mut root.children,
            stylesheet,
            &parent_for_children,
            root_font_px,
            ancestors,
            vw,
            vh,
            focused_box,
            keyboard_focus,
            &local_vars,
            candidates_buf,
            counters,
            hover_chain,
            share_cache,
            precomputed,
        );
    }

    ancestors.pop();

    // Pop counters that were reset at this level
    for name in counters_pushed.iter().rev() {
        if let Some(stack) = counters.get_mut(name) {
            stack.pop();
            if stack.is_empty() {
                counters.remove(name);
            }
        }
    }
}

fn apply_form_sizing_hints_after_ua(
    style: &mut ComputedStyle,
    root: &crate::types::WebCore,
    rules: &[CssRule],
    matched: &[(u32, usize)],
) {
    let author_declares = |property: &str| {
        matched
            .iter()
            .any(|(sp, ri)| is_author_origin(*sp) && rules[*ri].declarations.contains_key(property))
    };

    match root.tag.as_str() {
        "input" => {
            if !author_declares("width") {
                if let Some(size) = root.attributes.get("size") {
                    if let Ok(chars) = size.trim().parse::<f32>() {
                        if chars > 0.0 {
                            apply_property(style, "width", &format!("{}ch", chars));
                        }
                    }
                }
            }
        }
        "textarea" => {
            if !author_declares("height") {
                if let Some(rows) = root.attributes.get("rows") {
                    if let Ok(rows) = rows.trim().parse::<f32>() {
                        if rows > 0.0 {
                            apply_property(style, "height", &format!("{}em", rows * 1.4));
                        }
                    }
                }
            }
            if !author_declares("width") {
                if let Some(cols) = root.attributes.get("cols") {
                    if let Ok(cols) = cols.trim().parse::<f32>() {
                        if cols > 0.0 {
                            apply_property(style, "width", &format!("{}em", cols * 0.6));
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn collect_text_for_dir_auto(node: &WebCore) -> String {
    let mut out = String::new();
    collect_text_for_dir_auto_inner(node, &mut out);
    out
}

fn collect_text_for_dir_auto_inner(node: &WebCore, out: &mut String) {
    if matches!(node.tag.as_str(), "script" | "style") {
        return;
    }
    if node.tag != "#comment" && !node.text.is_empty() {
        out.push_str(&node.text);
    }
    for child in &node.children {
        collect_text_for_dir_auto_inner(child, out);
    }
}
