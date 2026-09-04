//! Collecting the text of an `aria-live` region.

#![allow(unused_imports)]
use super::*;
use crate::css::*;
use crate::dom::*;
use crate::html::*;
use std::collections::{HashMap, HashSet};

// ─── aria-live helper ──────────────────────────────────────────────────────────

/// Collect the visible text content of a live region by walking its subtree.
/// Used by `Document::check_live_regions` to compare snapshots.
pub(crate) fn collect_live_text(node: &WebCore) -> String {
    let mut buf = String::new();
    collect_live_text_inner(node, &mut buf);
    // Collapse runs of whitespace for stable comparison across minor reflows.
    let mut out = String::with_capacity(buf.len());
    let mut in_ws = false;
    for ch in buf.chars() {
        if ch.is_ascii_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            in_ws = false;
            out.push(ch);
        }
    }
    out.trim().to_string()
}

fn collect_live_text_inner(node: &WebCore, buf: &mut String) {
    if !node.text.trim().is_empty() {
        if !buf.is_empty() {
            buf.push(' ');
        }
        buf.push_str(node.text.trim());
    }
    for child in &node.children {
        if !matches!(child.style.display, Display::None) && child.style.visibility {
            collect_live_text_inner(child, buf);
        }
    }
}
