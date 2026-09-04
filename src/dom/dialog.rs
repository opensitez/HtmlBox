//! `HTMLDialogElement` — HTML §4.11.4.
//!
//! Modality is TOP-LAYER MEMBERSHIP, which lives in `top_layer.rs`; the UA
//! sheet's `dialog:modal` rule does the layout. Nothing here writes a style.

use crate::types::Document;

// ─── HTMLDialogElement ──────────────────────────────────────────────────────
//
// HTML §4.11.4. Openness is the `open` CONTENT ATTRIBUTE — the IDL property
// reflects it, so there is no separate "is it showing" flag to drift out of
// step, and markup that arrives with `<dialog open>` is already open without
// anyone calling `show()`.
//
// `display` and the non-modal `position` now come from the UA stylesheet
// (`dialog:not([open]) { display: none }` and the `dialog` block in
// `css::UA_CSS`), which is where the spec puts them. This file used to write
// both as INLINE styles because the sheet had no `dialog` entry — that made a
// dialog that had never been opened render in flow, and made an opened one
// immune to the author's own `display`, since an inline style beats every
// rule. Setting the attribute is the whole of `show()`; the cascade does the
// rest.
//
// `position` on a MODAL is the one thing still written here: a modal is laid
// out against the VIEWPORT and a non-modal stays with its containing block,
// and the difference is `dialog:modal`, a pseudo-class keyed on "is in the top
// layer" that the matcher has no state for yet. If both looked alike,
// `showModal()` would be `show()` under another name.

impl Document {
    /// `dialog.show()` / `dialog.showModal()`.
    pub fn show_dialog(&mut self, id: u32, modal: bool) {
        if id == 0 {
            return;
        }
        self.set_attribute(id, "open", "");
        // ⛔ The `position: fixed` INLINE style that used to live here is gone.
        // A modal is in the TOP LAYER, and the UA sheet's `dialog:modal` rule
        // does the layout — which means an author's own `position` can now
        // beat it, exactly as it already could for a non-modal dialog.
        if modal {
            self.add_to_top_layer(id, crate::types::TopLayerKind::ModalDialog);
        }
    }

    /// `dialog.close()`.
    pub fn close_dialog(&mut self, id: u32) {
        if id == 0 {
            return;
        }
        self.remove_attribute(id, "open");
        // Leaving the top layer is the whole of it: the `position` override a
        // modal used to carry was an inline style, and the UA sheet's
        // `dialog:modal` rule stops applying by itself.
        self.remove_from_top_layer(id);
    }

    /// `dialog.open` — reflects the content attribute, per the IDL.
    pub fn dialog_open(&self, id: u32) -> bool {
        id != 0 && self.get_attribute(id, "open").is_some()
    }
}
