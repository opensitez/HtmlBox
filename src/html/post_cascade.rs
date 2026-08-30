//! Fixups that need the cascade to have run.

#![allow(unused_imports)]
use super::*;
use crate::types::*;
use crate::css::*;

// ─── Post-cascade fixup ──────────────────────────────────────────────────────

/// After the CSS cascade runs, fix up `<summary>` display and `<details>` open/closed hiding.
/// The UA stylesheet sets `details, summary { display: block }` which overwrites our
/// parse-time settings, so we re-apply them here.
pub(crate) fn apply_details_summary_post_cascade(node: &mut WebCore) {
    if node.tag == "details" {
        let is_open = node.attributes.contains_key("open");
        for child in &mut node.children {
            if child.tag == "summary" {
                child.style.display = Display::ListItem;
                child.style.list_style_type = ListStyleType::Disclosure;
            } else if !is_open {
                child.style.display = Display::None;
            }
        }
    }

    for child in &mut node.children {
        apply_details_summary_post_cascade(child);
    }
}
