//! `aria-live` regions — collecting announcements when their content changes.

#![allow(unused_imports)]
use super::*;
use crate::css::*;
use std::collections::{HashMap, HashSet};
use crate::layout::LayoutEngine;
use crate::dom::*;
use crate::html::*;

impl Document {
    /// Drain and return all pending aria-live announcements.
    ///
    /// Call this after each layout pass and deliver the announcements to the
    /// platform (e.g. a screen reader via accesskit, a toast notification in a
    /// browser chrome, or a system alert).
    ///
    /// ```ignore
    /// for ann in doc.take_announcements() {
    ///     match ann.politeness {
    ///         LivePoliteness::Assertive => speak_immediately(&ann.text),
    ///         LivePoliteness::Polite    => speak_when_idle(&ann.text),
    ///         LivePoliteness::Off       => {}
    ///     }
    /// }
    /// ```
    pub fn take_announcements(&mut self) -> Vec<Announcement> {
        std::mem::take(&mut self.pending_announcements)
    }

    /// Scan all `aria-live` regions in the document, compare their text content
    /// to the snapshot from the previous call, and queue announcements for any
    /// regions whose content has changed.
    ///
    /// This is called automatically by `LayoutEngine::layout`.  You only need
    /// to call it manually if you modify the DOM outside of a layout pass.
    pub fn check_live_regions(&mut self) {
        let initialized = self.live_regions_initialized;
        let mut new_ann: Vec<Announcement> = Vec::new();

        fn walk(
            node:         &WebCore,
            snapshots:    &mut HashMap<u32, String>,
            out:          &mut Vec<Announcement>,
            initialized:  bool,
        ) {
            let politeness = match node.attributes.get("aria-live").map(|s| s.as_str()) {
                Some("assertive") => LivePoliteness::Assertive,
                Some("polite")    => LivePoliteness::Polite,
                _                 => LivePoliteness::Off,
            };

            if politeness != LivePoliteness::Off {
                // aria-busy: region is being updated, defer announcement
                let busy = node.attributes.get("aria-busy")
                    .map(|v| v == "true").unwrap_or(false);
                if !busy {
                    let ptr   = node.node_id;
                    let text  = collect_live_text(node);
                    let atomic = node.attributes.get("aria-atomic")
                        .map(|v| v == "true").unwrap_or(false);

                    match snapshots.get(&ptr) {
                        None => {
                            // First time seeing this region.
                            snapshots.insert(ptr, text.clone());
                            // Assertive regions announce on page load; polite ones
                            // are silently initialised so they don't flood the user.
                            if !initialized
                                && politeness == LivePoliteness::Assertive
                                && !text.is_empty()
                            {
                                out.push(Announcement { text, politeness, atomic });
                            }
                        }
                        Some(prev) if *prev != text => {
                            // Content changed since last layout pass.
                            let changed = text.clone();
                            snapshots.insert(ptr, text);
                            if !changed.is_empty() {
                                out.push(Announcement { text: changed, politeness, atomic });
                            }
                        }
                        _ => {} // No change, no announcement.
                    }
                }
                // Treat the live region as an atomic unit — don't recurse into it
                // looking for nested live regions (that would produce double announcements).
                return;
            }

            for child in &node.children {
                walk(child, snapshots, out, initialized);
            }
        }

        walk(&self.root, &mut self.live_region_snapshots, &mut new_ann, initialized);

        self.live_regions_initialized = true;
        self.pending_announcements.extend(new_ann);
    }
}
