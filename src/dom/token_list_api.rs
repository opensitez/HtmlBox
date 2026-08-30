//! The `Document` accessors over `DOMTokenList` — DOM §7.1, HTML §2.6.7.
//!
//! ⛔ Separate from `token_list.rs`, which holds the TYPES. Folding the
//! accessors in with them is the same layering confusion as free helper
//! functions sitting under an `impl` block.

use crate::types::Document;
use crate::dom::token_list::{TokenList, TokenListMut};

// ─── The token-list attributes — DOM §7.1, HTML §2.6.7 ──────────────────────
//
// Each of these is `DOMTokenList` over one attribute. They are named
// accessors rather than one `token_list(id, "class")` because the IDL names
// them, and because the supported-token set is per attribute: `relList`
// answers `supports`, `classList` throws for it.
impl Document {
    /// `element.classList`.
    pub fn class_list(&self, id: u32) -> TokenList<'_> {
        TokenList { doc: self, id, attr: "class" }
    }
    pub fn class_list_mut(&mut self, id: u32) -> TokenListMut<'_> {
        TokenListMut { doc: self, id, attr: "class" }
    }

    /// `relList` on `<a>`, `<area>`, `<link>` and `<form>`.
    pub fn rel_list(&self, id: u32) -> TokenList<'_> {
        TokenList { doc: self, id, attr: "rel" }
    }
    pub fn rel_list_mut(&mut self, id: u32) -> TokenListMut<'_> {
        TokenListMut { doc: self, id, attr: "rel" }
    }

    /// `iframe.sandbox`.
    pub fn sandbox(&self, id: u32) -> TokenList<'_> {
        TokenList { doc: self, id, attr: "sandbox" }
    }
    pub fn sandbox_mut(&mut self, id: u32) -> TokenListMut<'_> {
        TokenListMut { doc: self, id, attr: "sandbox" }
    }

    /// `element.part` — the shadow parts this element exposes to its host.
    pub fn part(&self, id: u32) -> TokenList<'_> {
        TokenList { doc: self, id, attr: "part" }
    }
    pub fn part_mut(&mut self, id: u32) -> TokenListMut<'_> {
        TokenListMut { doc: self, id, attr: "part" }
    }

    /// `output.htmlFor` — a token list of the ids this output was computed
    /// from. NOT the same member as `label.htmlFor`, which is a single id
    /// string; see [`html_for`](Self::html_for).
    pub fn html_for_list(&self, id: u32) -> TokenList<'_> {
        TokenList { doc: self, id, attr: "for" }
    }
    pub fn html_for_list_mut(&mut self, id: u32) -> TokenListMut<'_> {
        TokenListMut { doc: self, id, attr: "for" }
    }

    /// `label.htmlFor` / `script.htmlFor` — a single id, not a list.
    pub fn html_for(&self, id: u32) -> String {
        self.get_attribute(id, "for").unwrap_or_default()
    }
    pub fn set_html_for(&mut self, id: u32, value: &str) {
        self.set_attribute(id, "for", value);
    }
}
