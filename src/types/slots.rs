//! Assigning light-DOM children to shadow-tree slots.

#![allow(unused_imports)]
use super::*;
use std::collections::{HashMap, HashSet};
use crate::css::*;
use crate::dom::*;
use crate::html::*;

/// Resolve `<slot>` elements in a shadow tree by projecting light DOM children into them.
pub(crate) fn resolve_slots_inner(shadow_children: &mut Vec<WebCore>, light_children: &[WebCore]) {
    for child in shadow_children.iter_mut() {
        if child.tag == "slot" {
            let slot_name = child.attributes.get("name").cloned().unwrap_or_default();
            let projected: Vec<WebCore> = if slot_name.is_empty() {
                // Default slot: all light children without a `slot` attribute
                // Slottables are elements and non-blank text. A comment is
                // neither, so it is not projected.
                light_children.iter()
                    .filter(|lc| !lc.attributes.contains_key("slot") && lc.is_element()
                        || (lc.is_text_node() && !lc.text.trim().is_empty() && !lc.attributes.contains_key("slot")))
                    .cloned()
                    .collect()
            } else {
                // Named slot: light children with matching `slot` attribute
                light_children.iter()
                    .filter(|lc| lc.attributes.get("slot").map(|s| s == &slot_name).unwrap_or(false))
                    .cloned()
                    .collect()
            };
            if !projected.is_empty() {
                child.children = projected;
            }
            // If no matches, keep slot's own children as fallback
        } else {
            // Recurse into shadow tree children to find nested slots
            resolve_slots_inner(&mut child.children, light_children);
            // Also recurse into shadow roots of nested shadow hosts
            if let Some(ref mut sr) = child.shadow_root {
                resolve_slots_inner(&mut sr.children, light_children);
            }
        }
    }
}
