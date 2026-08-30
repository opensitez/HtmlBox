//! The incremental cascade — the `:hover` fast path and dirty-subtree walks.

#![allow(unused_imports)]
use std::collections::{HashMap, HashSet};
use rayon::prelude::*;
use crate::types::*;
use super::*;

// ─── Incremental Hover Cascade ──────────────────────────────────────────────

/// Mark nodes affected by a hover change by walking the tree.
/// Sets `cascade_dirty` on nodes whose hover state toggled (symmetric difference),
/// and `has_dirty_descendant` on their ancestors (the hover chain path).
pub fn mark_hover_dirty(
    root: &mut crate::types::WebCore,
    old_chain: &std::collections::HashSet<u32>,
    new_chain: &std::collections::HashSet<u32>,
    has_hover_descendant_rules: bool,
    hover_sensitive: &std::collections::HashSet<u32>,
) {
    // Nodes whose hover state actually changed (in one chain but not both)
    let toggled: std::collections::HashSet<u32> = old_chain.symmetric_difference(new_chain).copied().collect();
    // All nodes on the path (for has_dirty_descendant traversal)
    let path: std::collections::HashSet<u32> = old_chain.union(new_chain).copied().collect();

    fn walk(node: &mut crate::types::WebCore, toggled: &std::collections::HashSet<u32>,
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

fn mark_children_cascade_dirty(node: &mut crate::types::WebCore) {
    for child in &mut node.children {
        child.cascade_dirty = true;
        mark_children_cascade_dirty(child);
    }
}

/// Clear cascade_dirty and has_dirty_descendant flags after incremental cascade.
/// Clear cascade_dirty flags after cascade. Preserves has_dirty_descendant
/// for the layout pass (propagate_dirty uses it to skip clean subtrees).
pub fn clear_cascade_dirty(node: &mut crate::types::WebCore) {
    if !node.cascade_dirty && !node.has_dirty_descendant { return; }
    node.cascade_dirty = false;
    // Note: has_dirty_descendant is intentionally NOT cleared here — layout needs it.
    // It gets cleared after layout in clear_layout_dirty().
    for child in &mut node.children {
        clear_cascade_dirty(child);
    }
}

/// Clear has_dirty_descendant flags after layout completes.
pub fn clear_descendant_dirty(node: &mut crate::types::WebCore) {
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
    node: &mut crate::types::WebCore,
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
            inherited_vars, candidates_buf, counters, hover_chain, &[],
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
pub fn build_hover_chain(root: &crate::types::WebCore, target: u32) -> std::collections::HashSet<u32> {
    if target == 0 { return std::collections::HashSet::new(); }
    fn walk(node: &crate::types::WebCore, target: u32, path: &mut Vec<u32>) -> bool {
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
    root: &mut crate::types::WebCore,
    hover_chain: &std::collections::HashSet<u32>,
) -> bool {
    swap_hover_inner(root, hover_chain, false)
}

fn swap_hover_inner(
    node: &mut crate::types::WebCore,
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
            let other = node.style.hover_style.take().unwrap();
            // Preserve hover_style/active_style/visited_style from the base side
            let _hs_backup = node.style.hover_style.take(); // already None after take above
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
            // (swap_hover_inner path — simpler conditions than full cascade)
            let is_grid_or_flex = matches!(node.style.display,
                crate::types::Display::Grid | crate::types::Display::InlineGrid
                | crate::types::Display::Flex | crate::types::Display::InlineFlex);
            if !node.style.before_content.is_empty() {
                let before_is_positioned = node.style.before_style.as_ref().map_or(false, |ps|
                    matches!(ps.position, crate::types::Position::Absolute | crate::types::Position::Fixed));
                let before_is_block = node.style.before_style.as_ref().map_or(false, |ps|
                    ps.is_block_level());
                if is_grid_or_flex || before_is_positioned || before_is_block {
                    let existing = node.children.iter().position(|c| c.tag == "::before");
                    let mut pseudo_box = crate::types::WebCore::new("::before");
                    pseudo_box.text = node.style.before_content.clone();
                    if let Some(ref ps) = node.style.before_style {
                        pseudo_box.style = *ps.clone();
                    }
                    if is_grid_or_flex && !pseudo_box.style.is_positioned()
                        && matches!(pseudo_box.style.display, Display::Inline) {
                        pseudo_box.style.display = Display::Block;
                    }
                    if let Some(idx) = existing {
                        node.children[idx] = pseudo_box;
                    } else {
                        node.children.insert(0, pseudo_box);
                    }
                    node.style.before_content = String::new();
                }
            } else if let Some(idx) = node.children.iter().position(|c| c.tag == "::before") {
                node.children.remove(idx);
            }
            if !node.style.after_content.is_empty() {
                let after_is_positioned = node.style.after_style.as_ref().map_or(false, |ps|
                    matches!(ps.position, crate::types::Position::Absolute | crate::types::Position::Fixed));
                let after_is_block = node.style.after_style.as_ref().map_or(false, |ps|
                    ps.is_block_level());
                if is_grid_or_flex || after_is_positioned || after_is_block {
                    let existing = node.children.iter().position(|c| c.tag == "::after");
                    let mut pseudo_box = crate::types::WebCore::new("::after");
                    pseudo_box.text = node.style.after_content.clone();
                    if let Some(ref ps) = node.style.after_style {
                        pseudo_box.style = *ps.clone();
                    }
                    if is_grid_or_flex && !pseudo_box.style.is_positioned()
                        && matches!(pseudo_box.style.display, Display::Inline) {
                        pseudo_box.style.display = Display::Block;
                    }
                    if let Some(idx) = existing {
                        node.children[idx] = pseudo_box;
                    } else {
                        node.children.push(pseudo_box);
                    }
                    node.style.after_content = String::new();
                }
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
pub(crate) const MAX_CASCADE_DEPTH: usize = 400;

// ═══════════════════════════════════════════════════════════════════════════════
// Shared pseudo-element helpers — used by both sequential and parallel cascade
// ═══════════════════════════════════════════════════════════════════════════════
