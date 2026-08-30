//! `WebCore` — the render-tree node.

#![allow(unused_imports)]
use super::*;
use std::collections::{HashMap, HashSet};
use crate::css::*;
use crate::dom::*;
use crate::html::*;

// ─── HTML Box (DOM node) ─────────────────────────────────────────────────────

/// A box/node in the box tree.  Mirrors the C++ `Box` struct.
#[derive(Clone, Debug)]
pub struct WebCore {
    pub tag:        String,
    pub node_id:    u32,                // Stable identity — index into Document.nodes
    pub style:      ComputedStyle,
    pub attributes: crate::dom::attrs::AttrMap,
    pub text:       String,             // Own text content

    // ── Tree structure (linked-list children, O(1) insert/remove) ────────
    pub parent:       u32,              // 0 = no parent (root)
    pub first_child:  u32,              // 0 = no children
    pub last_child:   u32,              // 0 = no children
    pub next_sibling: u32,              // 0 = last child
    pub prev_sibling: u32,              // 0 = first child

    // DEPRECATED: Vec storage kept during migration. Will be removed.
    pub children:   Vec<WebCore>,

    /// Layout geometry — all layout-computed fields live here.
    pub layout: LayoutBox,

    // Custom component cached dimensions (set once by measure(), stable across relayouts).
    // Like replaced elements, components control their own size — the engine only
    // re-measures when the component is explicitly marked dirty.
    pub component_width:  f32,
    pub component_height: f32,

    // Image pixel data for <img> and replaced elements (RGBA8, row-major)
    /// Absolute URL this element's image was resolved to.
    ///
    /// A FIELD and not an attribute. It used to be stored as `_resolved_src` in
    /// `attributes`, which put an invented attribute on the WHATWG surface:
    /// `img.attributes` listed it, `getAttributeNames()` returned it, and it
    /// was serialized into the markup. Internal state may look however it
    /// likes, but not by pretending to be a content attribute.
    pub resolved_src: String,
    pub image_data:   Option<Vec<u8>>,
    pub image_width:  u32,
    pub image_height: u32,

    // Background image pixel data (RGBA8, row-major)
    pub bg_image_data:   Option<Vec<u8>>,
    pub bg_image_width:  u32,
    pub bg_image_height: u32,

    // CSS mask-image data (SVG rasterized to alpha mask)
    pub mask_image_data:   Option<Vec<u8>>,
    pub mask_image_width:  u32,
    pub mask_image_height: u32,

    // SVG source markup (for round-trip and re-rasterization)
    pub svg_markup: Option<String>,
    /// SVG viewBox intrinsic dimensions (width, height). Used for aspect ratio
    /// sizing in layout and on-demand rasterization at the correct display size.
    pub svg_viewbox_w: f32,
    pub svg_viewbox_h: f32,

    // ── Form input editing state ─────────────────────────────────────────
    /// **Checkedness** — whether the box is ticked RIGHT NOW.
    ///
    /// HTML §4.10.5.3 keeps this apart from the `checked` CONTENT ATTRIBUTE,
    /// which is `defaultChecked` — the value a form reset restores to. They
    /// start equal and diverge the moment anything ticks the box: a user
    /// clicking a checkbox must NOT rewrite the document, and
    /// `getAttribute("checked")` must keep answering what the markup says.
    ///
    /// This used to BE the attribute (`attributes.contains_key("checked")`), so
    /// clicking a box edited the page's own markup and a program reading the
    /// attribute back got the user's last click instead of its own default.
    /// One store cannot answer both questions, which is also why the reset
    /// algorithm was impossible to write.
    pub checkedness: bool,
    /// The **dirty checkedness flag** (HTML §4.10.5.3). Raised by a user
    /// interaction or by setting the `checked` IDL member; while it is false
    /// the content attribute still drives checkedness.
    pub dirty_checked: bool,
    /// **Selectedness** of an `<option>` (HTML §4.10.10) — the same separation
    /// `checkedness` draws, for the same reason. The `selected` CONTENT
    /// ATTRIBUTE is `defaultSelected`, the state a form reset restores to; this
    /// is what is selected right now.
    ///
    /// Selection lived in the parent `<select>`'s `data["_selected_idx"]`, a
    /// single index, so two things were inexpressible: a `multiple` list box
    /// with several rows picked, and a list box with NOTHING picked — which is
    /// not an edge case but the state HTML says a fresh list box is in, since
    /// the selectedness setting algorithm auto-selects only at display size 1.
    pub selectedness: bool,
    /// The **dirtiness** flag of an `<option>` (HTML §4.10.10). Raised by a
    /// user picking or toggling the option, and by the `selected` IDL setter;
    /// while it is false the content attribute still drives selectedness.
    pub dirty_selectedness: bool,
    /// The form control's **value** (HTML §4.10.18.1) when it has diverged from
    /// the `value` content attribute — `None` while they still agree.
    ///
    /// Third instance of the same shape. The `value` attribute is
    /// `defaultValue`; this is the value the control holds. Everything used to
    /// write the attribute, so typing into a field edited the document and a
    /// reset had nowhere to restore FROM — which is why a fictional
    /// `defaultValue` "content attribute" had been invented to hold the
    /// original. There is no such attribute in HTML.
    pub value_state: Option<String>,
    /// The **dirty value flag** (HTML §4.10.18.1). Once raised, the `value`
    /// content attribute no longer drives the value.
    pub dirty_value: bool,
    /// Cursor position (char index) within the input's value string.
    pub input_cursor: usize,
    /// Selection anchor (char index). When equal to input_cursor, no selection.
    pub input_sel_anchor: usize,
    /// `selectionDirection` (HTML §4.10.19.3).
    ///
    /// ⛔ Not derivable from `input_cursor` vs `input_sel_anchor`. A selection
    /// of (2,5) is reachable in THREE states — none, forward and backward —
    /// and Chrome reports all three: `setSelectionRange(2,5)` answers `"none"`
    /// while `setSelectionRange(2,5,"forward")` answers `"forward"`, with the
    /// same two offsets. The ordering of the pair can only carry two.
    pub input_sel_direction: SelectionDirection,
    /// Top-layer membership, or `None` when the element is not in it.
    pub top_layer_kind: Option<TopLayerKind>,

    // Custom data store (arbitrary key/value pairs set by application code)
    pub data: HashMap<String, String>,

    /// Matched CSS rules (populated only when inspect mode is enabled).
    /// Each entry records the selector, declarations, and source of a rule
    /// that matched this element during the cascade.
    pub matched_rules: Vec<MatchedRule>,

    /// Shadow DOM root. When present, layout/render use the shadow tree instead
    /// of `children` (which become "light DOM" — slottable content).
    pub shadow_root: Option<Box<ShadowRoot>>,

    /// True when hover_style has been swapped into the active `style` slot.
    /// Used by the fast hover-swap path to avoid full re-cascade on hover changes.
    pub hover_applied: bool,

    /// Set by `mark_hover_dirty()` before incremental cascade.
    /// True means this node's :hover match changed — must re-cascade.
    pub cascade_dirty: bool,
    /// True means a descendant has `cascade_dirty` — must traverse children.
    pub has_dirty_descendant: bool,
    /// True means a descendant has `layout_dirty` — must traverse into children during layout.
    /// Allows skipping entire clean subtrees.
    pub has_dirty_layout_descendant: bool,
}

/// Shadow DOM root — holds a scoped tree and stylesheet.
#[derive(Clone, Debug)]
pub struct ShadowRoot {
    /// The shadow tree nodes (laid out/painted instead of light DOM children).
    pub children: Vec<WebCore>,
    /// Scoped stylesheet — only applies inside this shadow tree.
    pub stylesheet: crate::css::Stylesheet,
    /// Open (inspectable) or closed (opaque).
    pub mode: ShadowMode,
    /// The shadow root's own node id.
    ///
    /// A `ShadowRoot` IS a node in the spec — a `DocumentFragment` — and
    /// `attachShadow` returns it. Without an id it could only be named through
    /// its host, so `delegatesFocus`, `activeElement`, `adoptedStyleSheets` and
    /// the rest had nothing to hang on.
    pub node_id: u32,
    /// `shadowRoot.delegatesFocus`.
    pub delegates_focus: bool,
    /// `shadowRoot.slotAssignment` — "named" (default) or "manual".
    pub slot_assignment: SlotAssignment,
    /// `shadowRoot.clonable`.
    pub clonable: bool,
    /// `shadowRoot.serializable`.
    pub serializable: bool,
    /// `shadowRoot.adoptedStyleSheets` — constructed sheets applied to the
    /// tree, kept as their source text.
    pub adopted_stylesheets: Vec<String>,
}

/// `ShadowRootInit.slotAssignment`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlotAssignment {
    /// Slots are matched by the `slot` attribute.
    #[default]
    Named,
    /// Slots are assigned explicitly through `slot.assign()`.
    Manual,
}

/// Which grammar a document was built from.
///
/// The DOM's own distinction, and the only thing it changes here is whether
/// names FOLD: HTML is ASCII-case-insensitive for tag and attribute names, XML
/// is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    Html,
    Xml,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShadowMode {
    Open,
    Closed,
}
