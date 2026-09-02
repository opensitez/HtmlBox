//! The parallel cascade pass.

#![allow(unused_imports)]
use std::collections::{HashMap, HashSet};
use rayon::prelude::*;
use crate::types::*;
use super::*;

// ─── Parallel Cascade ────────────────────────────────────────────────────────

/// Work item for the parallel cascade: one element extracted from the tree.
struct CascadeWorkItem {
    /// Path from root to this node (indices into children arrays).
    node_path: Vec<usize>,
    tag: String,
    attributes: crate::dom::attrs::AttrMap,
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
    node: &crate::types::WebCore,
    ancestors: &mut Vec<AncestorInfo>,
    path: &mut Vec<usize>,
    child_index: usize,
    sibling_count: usize,
    type_child_index: usize,
    type_sibling_count: usize,
    out: &mut Vec<CascadeWorkItem>,
) {
    if ancestors.len() >= MAX_CASCADE_DEPTH { return; }
    // Skip non-elements and pseudo-elements — they don't match CSS selectors.
    if !node.is_element() || node.tag == "::before" || node.tag == "::after" {
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
        let n_elem_children = node.children.iter().filter(|c| c.is_element()).count();
        let mut elem_pos = 0usize;
        let elem_indices: Vec<usize> = node.children.iter().map(|c| {
            if !c.is_element() { 0 } else { let p = elem_pos; elem_pos += 1; p }
        }).collect();

        for (i, child) in node.children.iter().enumerate() {
            let (ci, ns) = if !child.is_element() {
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
            prev_siblings: &[],
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
                let has_hover   = sel.has_hover;
                let has_active  = sel.has_active;
                let has_visited = sel.has_visited;

                if (has_hover || has_active || has_visited) && rule.pseudo_element == PseudoElement::None {
                    // Use pre-computed base_parts (no allocation per match)
                    if matches_selector_with_ancestors(
                        &sel.base_parts,
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
    root: &mut crate::types::WebCore,
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
            std::sync::Arc::make_mut(&mut root.style).inherit_from(p);
        }
        return;
    }

    if !root.is_element() {
        if let Some(p) = parent_style {
            std::sync::Arc::make_mut(&mut root.style).inherit_from(p);
        }
        return;
    }

    if root.tag == "::before" || root.tag == "::after" {
        if let Some(p) = parent_style {
            let saved_display = root.style.display;
            std::sync::Arc::make_mut(&mut root.style).inherit_from(p);
            std::sync::Arc::make_mut(&mut root.style).display = saved_display;
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

    let local_vars_owned = if has_new_vars || has_inline_vars {
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
        Some(vars)
    } else {
        None
    };
    let local_vars: &HashMap<String, String> = local_vars_owned.as_ref().unwrap_or(inherited_vars);

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
                apply_css_value_with_vars(&mut hs, id, val, &local_vars);
            }
        }
        for &(_, ri) in &hover_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
                apply_css_value_with_vars(&mut hs, id, val, &local_vars);
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
                apply_css_value_with_vars(&mut as_, id, val, &local_vars);
            }
        }
        for &(_, ri) in &active_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
                apply_css_value_with_vars(&mut as_, id, val, &local_vars);
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
                apply_css_value_with_vars(&mut vs, id, val, &local_vars);
            }
        }
        for &(_, ri) in &visited_matched {
            for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
                apply_css_value_with_vars(&mut vs, id, val, &local_vars);
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
        (Declarations::new(), Declarations::new())
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

    // `!important`, CSS Cascade §6.4.1 order: author sheet → inline → UA.
    for &(sp, ri) in &matched {
        if !is_author_origin(sp) { continue; }
        for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
            apply_css_value_with_vars(&mut style, id, val, &local_vars);
        }
    }
    for (prop, val) in &inline_important {
        let resolved = resolve_var_references(val, local_vars);
        if resolved.trim() == "inherit" {
            if let Some(p) = parent_style { copy_property_from_parent(&mut style, p, prop); }
        } else {
            apply_property(&mut style, prop, &resolved);
        }
    }
    for &(sp, ri) in &matched {
        if is_author_origin(sp) { continue; }
        for &(id, ref val) in &stylesheet.rules[ri].compiled_important {
            apply_css_value_with_vars(&mut style, id, val, &local_vars);
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
    root.style = std::sync::Arc::new(style.clone());

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
            std::sync::Arc::make_mut(&mut root.style).display = Display::Contents;
        }
    }

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

    if let Some((txt, ps)) = build_pseudo_style_shared(&mut before_matched, &root.style, local_vars, &stylesheet.rules) {
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
        std::sync::Arc::make_mut(&mut root.style).before_content = resolve_counters_in_content(&txt, counters);
        std::sync::Arc::make_mut(&mut root.style).before_style = Some(ps);
    }
    if let Some((txt, ps)) = build_pseudo_style_shared(&mut after_matched, &root.style, local_vars, &stylesheet.rules) {
        std::sync::Arc::make_mut(&mut root.style).after_content = resolve_counters_in_content(&txt, counters);
        std::sync::Arc::make_mut(&mut root.style).after_style = Some(ps);
    }
    if let Some((_, ps)) = build_pseudo_style_shared(&mut selection_matched, &root.style, local_vars, &stylesheet.rules) {
        std::sync::Arc::make_mut(&mut root.style).selection_style = Some(ps);
    }
    if let Some((_, ps)) = build_pseudo_style_shared(&mut marker_matched, &root.style, local_vars, &stylesheet.rules) {
        std::sync::Arc::make_mut(&mut root.style).marker_style = Some(ps);
    }

    // Create/update ::before and ::after child boxes
    {
        let is_grid_or_flex = matches!(root.style.display,
            Display::Grid | Display::InlineGrid | Display::Flex | Display::InlineFlex);
        let before_is_positioned = root.style.before_style.as_ref().map_or(false, |ps|
            matches!(ps.position, Position::Absolute | Position::Fixed));
        if (is_grid_or_flex && !root.style.before_content.is_empty())
            || (before_is_positioned && root.style.before_style.is_some())
        {
            let existing = root.children.iter().position(|c| c.tag == "::before");
            let mut pseudo_box = crate::types::WebCore::new("::before");
            pseudo_box.text = root.style.before_content.clone();
            pseudo_box.tag = "::before".to_string();
            if let Some(ref ps) = root.style.before_style {
                pseudo_box.style = std::sync::Arc::new(*ps.clone());
            }
            if is_grid_or_flex && !pseudo_box.style.is_positioned()
                && matches!(pseudo_box.style.display, Display::Inline) {
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
        let after_is_positioned = root.style.after_style.as_ref().map_or(false, |ps|
            matches!(ps.position, Position::Absolute | Position::Fixed));
        if (is_grid_or_flex && !root.style.after_content.is_empty())
            || (after_is_positioned && root.style.after_style.is_some())
        {
            let existing = root.children.iter().position(|c| c.tag == "::after");
            let mut pseudo_box = crate::types::WebCore::new("::after");
            pseudo_box.text = root.style.after_content.clone();
            pseudo_box.tag = "::after".to_string();
            if let Some(ref ps) = root.style.after_style {
                pseudo_box.style = std::sync::Arc::new(*ps.clone());
            }
            if is_grid_or_flex && !pseudo_box.style.is_positioned()
                && matches!(pseudo_box.style.display, Display::Inline) {
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
    children: &mut [crate::types::WebCore],
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
    let n_elem_children = children.iter().filter(|c| c.is_element()).count();
    let mut elem_pos = 0usize;
    let elem_indices: Vec<usize> = children.iter().map(|c| {
        if !c.is_element() { 0 } else { let p = elem_pos; elem_pos += 1; p }
    }).collect();
    for (i, child) in children.iter_mut().enumerate() {
        let (ci, ns) = if !child.is_element() {
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
    children: &mut [crate::types::WebCore],
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
    let n_elem_children = children.iter().filter(|c| c.is_element()).count();
    let mut elem_pos = 0usize;
    let elem_indices: Vec<usize> = children.iter().map(|c| {
        if !c.is_element() { 0 } else { let p = elem_pos; elem_pos += 1; p }
    }).collect();
    for (i, child) in children.iter_mut().enumerate() {
        let (ci, ns) = if !child.is_element() {
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
            hover_chain, &[],
            // A cache local to this call: an incremental re-cascade
            // touches one subtree, so nothing outside it can be shared into.
            &mut crate::css::cascade::ShareCache::new(),
        );
    }
}

/// Parallel cascade: 3-pass approach for large stylesheets.
/// 1. Flatten DOM into work list with ancestor snapshots (sequential)
/// 2. Run selector matching in parallel via Rayon (parallel)
/// 3. Apply matched rules to styles (sequential — inherits from parent, builds state styles)
pub fn apply_cascade_parallel(
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
