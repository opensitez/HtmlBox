//! Accessibility tree builder for webcore.
//!
//! Converts the live `Document` box tree into an `accesskit::TreeUpdate` that can
//! be pushed to an `accesskit_winit::Adapter`.  Every visible `WebCore` becomes
//! one `accesskit::Node`; the node's `NodeId` is the box's raw pointer cast to
//! `u64 | 1` (guaranteeing non-zero).
//!
//! # Usage
//! ```ignore
//! // After every layout rebuild:
//! let update = webcore::accessibility::build_tree(&doc, platform.scale_factor());
//! adapter.update_if_active(|| update);
//! ```
//!
//! # EventLoop setup (accesskit_winit)
//! `accesskit_winit` requires the winit event loop to carry its action-request
//! event type.  Create the loop with:
//! ```ignore
//! use accesskit_winit::ActionRequestEvent;
//! let event_loop = EventLoop::<ActionRequestEvent>::with_user_event().build().unwrap();
//! let proxy = event_loop.create_proxy();
//! let adapter = accesskit_winit::Adapter::new(&window,
//!     || webcore::accessibility::build_tree(&doc, scale),
//!     proxy);
//!
//! // In your event handler — call for every WindowEvent:
//! adapter.process_event(&window, &event);
//!
//! // Handle action requests (focus requests from screen readers, etc.):
//! Event::UserEvent(ActionRequestEvent { request, .. }) => {
//!     if request.action == accesskit::Action::Focus {
//!         // The NodeId encodes the WebCore pointer: id.0 & !1 gives the address.
//!         // Find the box, set doc.focused_box, fire Focus events, request redraw.
//!     }
//! }
//! ```

#[cfg(feature = "accessibility")]
use crate::types::{Display, Document, WebCore};
#[cfg(feature = "accessibility")]
use accesskit::{
    Action, AutoComplete, HasPopup, Invalid, Live, Node, NodeId, Orientation, Rect as AkRect, Role,
    SortDirection, Toggled, Tree, TreeUpdate,
};
#[cfg(feature = "accessibility")]
use std::collections::HashMap;

/// Synthetic document-root NodeId (wraps the real root element).
#[cfg(feature = "accessibility")]
const ROOT_ID: NodeId = NodeId(1);

/// Convert a node_id to a stable, non-zero `NodeId`.
/// Setting bit 0 guarantees non-zero.
#[cfg(feature = "accessibility")]
#[inline]
fn nid_to_ak(node_id: u32) -> NodeId {
    NodeId((node_id as u64) | 1)
}

/// Build a complete `accesskit::TreeUpdate` from the current document state.
///
/// `scale` is the HiDPI scale factor (`platform.scale_factor()`); it is applied
/// to layout coordinates so the platform accessibility layer receives correct
/// physical-pixel bounds.
///
/// **Call this after every layout rebuild** — NodeIds encode pointer addresses
/// that change when the box tree is reconstructed.
#[cfg(feature = "accessibility")]
pub fn build_tree(doc: &Document, scale: f32) -> TreeUpdate {
    // Pre-pass 1: id→NodeId and id→text for aria-labelledby/describedby/controls/owns.
    let mut id_to_nid: HashMap<&str, NodeId> = HashMap::new();
    let mut id_to_text: HashMap<&str, String> = HashMap::new();
    collect_id_maps(&doc.root, &mut id_to_nid, &mut id_to_text);

    // Pre-pass 2: <label> associations.
    //   for_label_map:     element-id → label text  (from <label for="id">)
    //   wrapped_label_map: input-ptr  → label text  (from <label> wrapping the control)
    let mut for_label_map: HashMap<&str, String> = HashMap::new();
    let mut wrapped_label_map: HashMap<u32, String> = HashMap::new();
    collect_label_maps(&doc.root, &mut for_label_map, &mut wrapped_label_map);

    let mut nodes: Vec<(NodeId, Node)> = Vec::new();
    let focused_id = doc.focused_box;

    // Walk the real root element.
    let real_root_id = walk(
        &doc.root,
        scale,
        &mut nodes,
        focused_id,
        &id_to_nid,
        &id_to_text,
        &for_label_map,
        &wrapped_label_map,
    );

    // Synthetic document root that owns the real root element.
    let mut doc_root = Node::new(Role::Window);
    doc_root.set_children(vec![real_root_id]);
    nodes.push((ROOT_ID, doc_root));

    // AccessKit focus: use the focused box's id, fall back to root.
    let focus_id = if focused_id == 0 {
        ROOT_ID
    } else {
        nid_to_ak(focused_id)
    };

    TreeUpdate {
        nodes,
        tree: Some(Tree::new(ROOT_ID)),
        focus: focus_id,
    }
}

/// Pre-pass: collect every element with an `id` attribute into two lookup maps.
#[cfg(feature = "accessibility")]
fn collect_id_maps<'a>(
    node: &'a WebCore,
    id_to_nid: &mut HashMap<&'a str, NodeId>,
    id_to_text: &mut HashMap<&'a str, String>,
) {
    if let Some(id) = node.attributes.get("id") {
        if !id.is_empty() {
            id_to_nid.insert(id.as_str(), nid_to_ak(node.node_id));
            id_to_text.insert(id.as_str(), collect_text(node));
        }
    }
    for child in &node.children {
        collect_id_maps(child, id_to_nid, id_to_text);
    }
}

/// Pre-pass: collect `<label>` associations.
///
/// Two forms of HTML label association:
/// - `<label for="id">` → stores `id → label_text` in `for_label_map`
/// - `<label>` wrapping a control → stores `control_ptr → label_text` in `wrapped_label_map`
#[cfg(feature = "accessibility")]
fn collect_label_maps<'a>(
    node: &'a WebCore,
    for_label_map: &mut HashMap<&'a str, String>,
    wrapped_label_map: &mut HashMap<u32, String>,
) {
    if node.tag == "label" {
        // Collect the label's visible text, excluding embedded controls whose
        // own text would confusingly appear in their own name.
        let text = collect_label_text(node);
        if !text.is_empty() {
            if let Some(for_id) = node.attributes.get("for") {
                if !for_id.is_empty() {
                    for_label_map.insert(for_id.as_str(), text);
                }
            } else if let Some(nid) = find_labelable_descendant(node) {
                wrapped_label_map.insert(nid, text);
            }
        }
    }
    for child in &node.children {
        collect_label_maps(child, for_label_map, wrapped_label_map);
    }
}

/// Collect text from a `<label>`, skipping embedded labelable controls so they
/// don't contribute their own placeholder/value to the computed name.
#[cfg(feature = "accessibility")]
fn collect_label_text(node: &WebCore) -> String {
    let mut s = node.text.trim().to_string();
    for child in &node.children {
        if matches!(
            child.tag.as_str(),
            "input" | "textarea" | "select" | "button"
        ) {
            continue; // skip embedded controls
        }
        if !matches!(child.style.display, Display::None) && child.style.visibility {
            let ct = collect_label_text(child);
            if !ct.is_empty() {
                if !s.is_empty() {
                    s.push(' ');
                }
                s.push_str(&ct);
            }
        }
    }
    s.trim().to_string()
}

/// Find the first labelable descendant (for implicit `<label>` wrapping).
/// Returns the raw pointer to the control so it can be used as a map key.
#[cfg(feature = "accessibility")]
fn find_labelable_descendant(node: &WebCore) -> Option<u32> {
    for child in &node.children {
        let tag = child.tag.as_str();
        let labelable = matches!(
            tag,
            "button" | "input" | "meter" | "output" | "progress" | "select" | "textarea"
        ) && child
            .attributes
            .get("type")
            .map(|t| t != "hidden")
            .unwrap_or(true);
        if labelable {
            return Some(child.node_id);
        }
        if let Some(nid) = find_labelable_descendant(child) {
            return Some(nid);
        }
    }
    None
}

/// Resolve a space-separated list of idrefs (aria-labelledby etc.) to NodeIds.
#[cfg(feature = "accessibility")]
fn resolve_idrefs<'a>(refs: &str, id_to_nid: &HashMap<&'a str, NodeId>) -> Vec<NodeId> {
    refs.split_whitespace()
        .filter_map(|id| id_to_nid.get(id).copied())
        .collect()
}

/// Collect the text content of a space-separated idref list.
#[cfg(feature = "accessibility")]
fn text_from_idrefs(refs: &str, id_to_text: &HashMap<&str, String>) -> String {
    refs.split_whitespace()
        .filter_map(|id| id_to_text.get(id))
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Recursively walk `node` and all visible descendants, appending to `nodes`.
/// Returns the `NodeId` assigned to `node`.
#[cfg(feature = "accessibility")]
fn walk(
    node: &WebCore,
    scale: f32,
    nodes: &mut Vec<(NodeId, Node)>,
    focused_id: u32,
    id_to_nid: &HashMap<&str, NodeId>,
    id_to_text: &HashMap<&str, String>,
    for_label_map: &HashMap<&str, String>,
    wrapped_label_map: &HashMap<u32, String>,
) -> NodeId {
    let id = nid_to_ak(node.node_id);
    let role = resolve_role(node);
    let mut ak = Node::new(role);

    // ── Bounds (physical pixels) ──────────────────────────────────────────────
    let r = &node.layout.border_rect;
    ak.set_bounds(AkRect {
        x0: (r.x * scale) as f64,
        y0: (r.y * scale) as f64,
        x1: ((r.x + r.w) * scale) as f64,
        y1: ((r.y + r.h) * scale) as f64,
    });

    // ── Accessible name ───────────────────────────────────────────────────────
    // aria-labelledby overrides everything; falls back to compute_name().
    if let Some(refs) = node.attributes.get("aria-labelledby") {
        let text = text_from_idrefs(refs, id_to_text);
        if !text.is_empty() {
            ak.set_label(text);
            let nids = resolve_idrefs(refs, id_to_nid);
            if !nids.is_empty() {
                ak.set_labelled_by(nids);
            }
        } else if let Some(name) = compute_name(node, id_to_text, for_label_map, wrapped_label_map)
        {
            ak.set_label(name);
        }
    } else if let Some(name) = compute_name(node, id_to_text, for_label_map, wrapped_label_map) {
        ak.set_label(name);
    }

    // ── Description (aria-describedby / aria-description / title) ─────────────
    if let Some(refs) = node.attributes.get("aria-describedby") {
        let text = text_from_idrefs(refs, id_to_text);
        if !text.is_empty() {
            ak.set_description(text.as_str());
            let nids = resolve_idrefs(refs, id_to_nid);
            if !nids.is_empty() {
                ak.set_described_by(nids);
            }
        }
    } else if let Some(desc) = node
        .attributes
        .get("aria-description")
        .or_else(|| node.attributes.get("title"))
    {
        if !desc.is_empty() {
            ak.set_description(desc.as_str());
        }
    }

    // ── aria-controls ─────────────────────────────────────────────────────────
    if let Some(refs) = node.attributes.get("aria-controls") {
        let nids = resolve_idrefs(refs, id_to_nid);
        if !nids.is_empty() {
            ak.set_controls(nids);
        }
    }

    // ── aria-owns ─────────────────────────────────────────────────────────────
    if let Some(refs) = node.attributes.get("aria-owns") {
        let nids = resolve_idrefs(refs, id_to_nid);
        if !nids.is_empty() {
            ak.set_owns(nids);
        }
    }

    // ── Disabled ──────────────────────────────────────────────────────────────
    if node.attributes.contains_key("disabled")
        || node
            .attributes
            .get("aria-disabled")
            .map(|v| v == "true")
            .unwrap_or(false)
    {
        ak.set_disabled();
    }

    // ── Hidden (screen-reader invisible) ─────────────────────────────────────
    if node
        .attributes
        .get("aria-hidden")
        .map(|v| v == "true")
        .unwrap_or(false)
    {
        ak.set_hidden();
    }

    // ── Expanded ──────────────────────────────────────────────────────────────
    if let Some(v) = node.attributes.get("aria-expanded") {
        ak.set_expanded(v == "true");
    }

    // ── Checked / toggled (checkbox, radio, switch) ───────────────────────────
    let checked_attr = node
        .attributes
        .get("aria-checked")
        .map(|s| s.as_str())
        // A screen reader announces the CURRENT state, so this is checkedness
        // and not the markup's default — the same reason the painter reads it.
        .or_else(|| if node.checkedness { Some("true") } else { None });
    match checked_attr {
        Some("true") => ak.set_toggled(Toggled::True),
        Some("mixed") => ak.set_toggled(Toggled::Mixed),
        Some("false") => ak.set_toggled(Toggled::False),
        _ => {}
    }

    // ── Selected ──────────────────────────────────────────────────────────────
    if let Some(v) = node.attributes.get("aria-selected") {
        ak.set_selected(v == "true");
    }

    // ── Required ──────────────────────────────────────────────────────────────
    if node.attributes.contains_key("required")
        || node
            .attributes
            .get("aria-required")
            .map(|v| v == "true")
            .unwrap_or(false)
    {
        ak.set_required();
    }

    // ── Read-only ─────────────────────────────────────────────────────────────
    if node.attributes.contains_key("readonly")
        || node
            .attributes
            .get("aria-readonly")
            .map(|v| v == "true")
            .unwrap_or(false)
    {
        ak.set_read_only();
    }

    // ── Multiselectable ───────────────────────────────────────────────────────
    if node
        .attributes
        .get("aria-multiselectable")
        .map(|v| v == "true")
        .unwrap_or(false)
    {
        ak.set_multiselectable();
    }

    // ── Modal ─────────────────────────────────────────────────────────────────
    if node
        .attributes
        .get("aria-modal")
        .map(|v| v == "true")
        .unwrap_or(false)
    {
        ak.set_modal();
    }

    // ── Busy ──────────────────────────────────────────────────────────────────
    if node
        .attributes
        .get("aria-busy")
        .map(|v| v == "true")
        .unwrap_or(false)
    {
        ak.set_busy();
    }

    // ── Invalid ───────────────────────────────────────────────────────────────
    if let Some(inv) = node.attributes.get("aria-invalid") {
        let state = match inv.as_str() {
            "grammar" => Some(Invalid::Grammar),
            "spelling" => Some(Invalid::Spelling),
            "true" => Some(Invalid::True),
            _ => None,
        };
        if let Some(s) = state {
            ak.set_invalid(s);
        }
    }

    // ── Has-popup ─────────────────────────────────────────────────────────────
    if let Some(hp) = node.attributes.get("aria-haspopup") {
        let val = match hp.as_str() {
            "menu" => Some(HasPopup::Menu),
            "listbox" => Some(HasPopup::Listbox),
            "tree" => Some(HasPopup::Tree),
            "grid" => Some(HasPopup::Grid),
            "dialog" => Some(HasPopup::Dialog),
            "true" => Some(HasPopup::True),
            _ => None,
        };
        if let Some(v) = val {
            ak.set_has_popup(v);
        }
    }

    // ── Autocomplete ──────────────────────────────────────────────────────────
    if let Some(ac) = node.attributes.get("aria-autocomplete") {
        let val = match ac.as_str() {
            "inline" => Some(AutoComplete::Inline),
            "list" => Some(AutoComplete::List),
            "both" => Some(AutoComplete::Both),
            _ => None,
        };
        if let Some(v) = val {
            ak.set_auto_complete(v);
        }
    }

    // ── Heading level ─────────────────────────────────────────────────────────
    if matches!(node.tag.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
        let level = node.tag[1..].parse::<usize>().unwrap_or(1);
        ak.set_level(level);
    }
    if let Some(level_str) = node.attributes.get("aria-level") {
        if let Ok(level) = level_str.parse::<usize>() {
            ak.set_level(level);
        }
    }

    // ── Orientation ───────────────────────────────────────────────────────────
    if let Some(orient) = node.attributes.get("aria-orientation") {
        match orient.as_str() {
            "horizontal" => ak.set_orientation(Orientation::Horizontal),
            "vertical" => ak.set_orientation(Orientation::Vertical),
            _ => {}
        }
    }

    // ── Sort direction ────────────────────────────────────────────────────────
    if let Some(sort) = node.attributes.get("aria-sort") {
        let dir = match sort.as_str() {
            "ascending" => Some(SortDirection::Ascending),
            "descending" => Some(SortDirection::Descending),
            "other" => Some(SortDirection::Other),
            _ => None,
        };
        if let Some(d) = dir {
            ak.set_sort_direction(d);
        }
    }

    // ── Numeric value (sliders, spinbuttons, meters, progress) ───────────────
    if let Some(v) = node
        .attributes
        .get("aria-valuenow")
        .and_then(|s| s.parse::<f64>().ok())
    {
        ak.set_numeric_value(v);
    }
    if let Some(v) = node
        .attributes
        .get("aria-valuemin")
        .and_then(|s| s.parse::<f64>().ok())
    {
        ak.set_min_numeric_value(v);
    }
    if let Some(v) = node
        .attributes
        .get("aria-valuemax")
        .and_then(|s| s.parse::<f64>().ok())
    {
        ak.set_max_numeric_value(v);
    }
    // aria-valuetext overrides numeric value for screen-reader announcements.
    if let Some(vt) = node.attributes.get("aria-valuetext") {
        if !vt.is_empty() {
            ak.set_value(vt.as_str());
        }
    } else if matches!(node.tag.as_str(), "input" | "textarea") {
        // The VALUE a screen reader announces is what the control HOLDS. The
        // `value` attribute is its default, so this used to read out the
        // author's placeholder number after the user had moved the control.
        let val = crate::types::input_value(node);
        if !val.is_empty() {
            ak.set_value(val.as_str());
        }
    } else if let Some(val) = node.attributes.get("value") {
        ak.set_value(val.as_str());
    }

    // ── Set size / position in set (listbox options, tabs, …) ────────────────
    if let Some(v) = node
        .attributes
        .get("aria-setsize")
        .and_then(|s| s.parse::<usize>().ok())
    {
        ak.set_size_of_set(v);
    }
    if let Some(v) = node
        .attributes
        .get("aria-posinset")
        .and_then(|s| s.parse::<usize>().ok())
    {
        ak.set_position_in_set(v);
    }

    // ── Table / grid dimensions ───────────────────────────────────────────────
    if let Some(v) = node
        .attributes
        .get("aria-rowcount")
        .and_then(|s| s.parse::<usize>().ok())
    {
        ak.set_row_count(v);
    }
    if let Some(v) = node
        .attributes
        .get("aria-colcount")
        .and_then(|s| s.parse::<usize>().ok())
    {
        ak.set_column_count(v);
    }
    if let Some(v) = node
        .attributes
        .get("aria-rowindex")
        .and_then(|s| s.parse::<usize>().ok())
    {
        ak.set_row_index(v);
    }
    if let Some(v) = node
        .attributes
        .get("aria-colindex")
        .and_then(|s| s.parse::<usize>().ok())
    {
        ak.set_column_index(v);
    }
    if let Some(v) = node
        .attributes
        .get("aria-rowspan")
        .and_then(|s| s.parse::<usize>().ok())
    {
        ak.set_row_span(v);
    }
    if let Some(v) = node
        .attributes
        .get("aria-colspan")
        .and_then(|s| s.parse::<usize>().ok())
    {
        ak.set_column_span(v);
    }

    // ── Placeholder ───────────────────────────────────────────────────────────
    if let Some(ph) = node.attributes.get("placeholder") {
        if !ph.is_empty() {
            ak.set_placeholder(ph.as_str());
        }
    }

    // ── URL (links) ───────────────────────────────────────────────────────────
    if node.tag == "a" {
        if let Some(href) = node.attributes.get("href") {
            ak.set_url(href.as_str());
        }
    }

    // ── aria-live regions ─────────────────────────────────────────────────────
    if let Some(live) = node.attributes.get("aria-live") {
        let live_val = match live.as_str() {
            "polite" => Live::Polite,
            "assertive" => Live::Assertive,
            _ => Live::Off,
        };
        ak.set_live(live_val);
    }
    if node
        .attributes
        .get("aria-atomic")
        .map(|v| v == "true")
        .unwrap_or(false)
    {
        ak.set_live_atomic();
    }

    // ── Supported actions ─────────────────────────────────────────────────────
    if is_focusable(node) {
        ak.add_action(Action::Focus);
    }
    if matches!(
        role,
        Role::Button
            | Role::DefaultButton
            | Role::Link
            | Role::CheckBox
            | Role::RadioButton
            | Role::Switch
            | Role::MenuItem
            | Role::MenuItemCheckBox
    ) {
        ak.add_action(Action::Click);
    }
    if matches!(
        role,
        Role::TextInput | Role::MultilineTextInput | Role::SearchInput
    ) {
        ak.add_action(Action::SetTextSelection);
    }
    if matches!(role, Role::ScrollView | Role::Application) {
        ak.add_action(Action::ScrollDown);
        ak.add_action(Action::ScrollUp);
    }

    // ── Children (skip display:none and aria-hidden subtrees) ─────────────────
    let child_ids: Vec<NodeId> = node
        .children
        .iter()
        .filter(|c| {
            !matches!(c.style.display, Display::None)
                && c.style.visibility
                && !c
                    .attributes
                    .get("aria-hidden")
                    .map(|v| v == "true")
                    .unwrap_or(false)
        })
        .map(|c| {
            walk(
                c,
                scale,
                nodes,
                focused_id,
                id_to_nid,
                id_to_text,
                for_label_map,
                wrapped_label_map,
            )
        })
        .collect();
    if !child_ids.is_empty() {
        ak.set_children(child_ids);
    }

    nodes.push((id, ak));
    id
}

/// Resolve the ARIA role: explicit `role` attribute → HTML element semantics.
#[cfg(feature = "accessibility")]
fn resolve_role(node: &WebCore) -> Role {
    if let Some(role_attr) = node.attributes.get("role") {
        match role_attr.as_str() {
            "button" => return Role::Button,
            "link" => return Role::Link,
            "heading" => return Role::Heading,
            "checkbox" => return Role::CheckBox,
            "radio" => return Role::RadioButton,
            "switch" => return Role::Switch,
            "textbox" => return Role::TextInput,
            "searchbox" => return Role::SearchInput,
            "combobox" => return Role::ComboBox,
            "listbox" => return Role::ListBox,
            "option" => return Role::ListBoxOption,
            "list" => return Role::List,
            "listitem" => return Role::ListItem,
            "menu" => return Role::Menu,
            "menuitem" => return Role::MenuItem,
            "menuitemcheckbox" => return Role::MenuItemCheckBox,
            "menuitemradio" => return Role::MenuItemRadio,
            "radiogroup" => return Role::RadioGroup,
            "menubar" => return Role::MenuBar,
            "toolbar" => return Role::Toolbar,
            "tooltip" => return Role::Tooltip,
            "navigation" => return Role::Navigation,
            "main" => return Role::Main,
            "banner" => return Role::Banner,
            "contentinfo" => return Role::ContentInfo,
            "complementary" => return Role::Complementary,
            "region" => return Role::Region,
            "article" => return Role::Article,
            "form" => return Role::Form,
            "search" => return Role::Search,
            "dialog" => return Role::Dialog,
            "alertdialog" => return Role::AlertDialog,
            "alert" => return Role::Alert,
            "status" => return Role::Status,
            "log" => return Role::Log,
            "marquee" => return Role::Marquee,
            "timer" => return Role::Timer,
            "progressbar" => return Role::ProgressIndicator,
            "slider" => return Role::Slider,
            "spinbutton" => return Role::SpinButton,
            "scrollbar" => return Role::ScrollBar,
            "tab" => return Role::Tab,
            "tablist" => return Role::TabList,
            "tabpanel" => return Role::TabPanel,
            "table" => return Role::Table,
            "row" => return Role::Row,
            "cell" => return Role::Cell,
            "columnheader" => return Role::ColumnHeader,
            "rowheader" => return Role::RowHeader,
            "rowgroup" => return Role::RowGroup,
            "grid" => return Role::Grid,
            "treegrid" => return Role::TreeGrid,
            "tree" => return Role::Tree,
            "treeitem" => return Role::TreeItem,
            "separator" => return Role::Splitter,
            "img" => return Role::Image,
            "figure" => return Role::Figure,
            "group" => return Role::Group,
            "math" => return Role::Math,
            "feed" => return Role::Feed,
            "mark" => return Role::Mark,
            "note" => return Role::Note,
            "term" => return Role::Term,
            "definition" => return Role::Definition,
            "directory" => return Role::Directory,
            "doc-abstract" => return Role::DocAbstract,
            "none" | "presentation" => return Role::GenericContainer,
            "generic" => return Role::GenericContainer,
            _ => {} // fall through to element semantics
        }
    }

    let input_type = || node.attributes.get("type").map(|s| s.as_str());

    match node.tag.as_str() {
        "button" => Role::Button,
        "a" if node.attributes.contains_key("href") => Role::Link,
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Role::Heading,
        "input" => match input_type() {
            Some("checkbox") => Role::CheckBox,
            Some("radio") => Role::RadioButton,
            Some("button") | Some("submit") | Some("reset") => Role::Button,
            Some("range") => Role::Slider,
            Some("number") => Role::SpinButton,
            Some("search") => Role::SearchInput,
            Some("email") => Role::EmailInput,
            Some("url") => Role::UrlInput,
            Some("tel") => Role::PhoneNumberInput,
            Some("password") => Role::PasswordInput,
            Some("color") => Role::ColorWell,
            Some("date") => Role::DateInput,
            Some("datetime-local") => Role::DateTimeInput,
            Some("week") => Role::WeekInput,
            Some("month") => Role::MonthInput,
            Some("time") => Role::TimeInput,
            _ => Role::TextInput,
        },
        "textarea" => Role::MultilineTextInput,
        "select" => Role::ComboBox,
        "option" => Role::ListBoxOption,
        "optgroup" => Role::Group,
        "img" => Role::Image,
        "figure" => Role::Figure,
        "figcaption" => Role::FigureCaption,
        "ul" | "ol" => Role::List,
        "li" => Role::ListItem,
        "dl" => Role::DescriptionList,
        "dt" => Role::DescriptionListTerm,
        "dd" => Role::DescriptionListDetail,
        "nav" => Role::Navigation,
        "main" => Role::Main,
        // <header>/<footer> are landmarks only at document scope; inside
        // article/section they are non-landmark.  Without parent context we
        // default to landmark and rely on the ARIA role attribute to override.
        "header" => Role::Banner,
        "footer" => Role::ContentInfo,
        "aside" => Role::Complementary,
        "section" => Role::Region,
        "article" => Role::Article,
        "form" => Role::Form,
        "search" => Role::Search,
        "dialog" => Role::Dialog,
        "menu" => Role::Menu,
        "menuitem" => Role::MenuItem,
        "table" => Role::Table,
        "thead" | "tbody" | "tfoot" => Role::RowGroup,
        "tr" => Role::Row,
        "td" => Role::Cell,
        "th" => Role::ColumnHeader,
        "caption" => Role::Caption,
        "details" => Role::Details,
        "summary" => Role::DisclosureTriangle,
        "meter" => Role::Meter,
        "progress" => Role::ProgressIndicator,
        "hr" => Role::Splitter,
        "blockquote" => Role::Blockquote,
        "pre" => Role::Pre,
        "code" => Role::Code,
        "del" | "s" => Role::ContentDeletion,
        "ins" => Role::ContentInsertion,
        "mark" => Role::Mark,
        "em" | "i" => Role::Emphasis,
        "strong" | "b" => Role::Strong,
        "abbr" => Role::Abbr,
        "dfn" => Role::Term,
        "time" => Role::Time,
        "ruby" => Role::Ruby,
        "output" => Role::Status,
        "body" => Role::Document,
        "html" => Role::Document,
        "label" => Role::Label,
        "legend" => Role::Legend,
        "fieldset" => Role::Group,
        "address" => Role::Group,
        "audio" => Role::Audio,
        "video" => Role::Video,
        "canvas" => Role::Canvas,
        "iframe" => Role::Iframe,
        "br" => Role::LineBreak,
        "p" => Role::Paragraph,
        _ => Role::GenericContainer,
    }
}

/// Compute the accessible name following the AccName algorithm
/// (https://www.w3.org/TR/accname-1.1/), simplified for HTML elements.
///
/// Priority order:
/// 1. `aria-labelledby` (handled by caller — already resolved before this is called)
/// 2. `aria-label`
/// 3. `<label for="id">` or wrapping `<label>`
/// 4. Element-specific native name: `alt` for images, child caption/legend/figcaption
/// 5. Name-from-content (buttons, links, headings…)
/// 6. `placeholder` for unlabelled text inputs
/// 7. `title` as last resort
#[cfg(feature = "accessibility")]
fn compute_name(
    node: &WebCore,
    id_to_text: &HashMap<&str, String>,
    for_label_map: &HashMap<&str, String>,
    wrapped_label_map: &HashMap<u32, String>,
) -> Option<String> {
    // 1. aria-labelledby handled by caller before reaching here.

    // 2. aria-label
    if let Some(label) = node.attributes.get("aria-label") {
        if !label.is_empty() {
            return Some(label.clone());
        }
    }

    // 3a. <label for="this-id">
    if let Some(id) = node.attributes.get("id") {
        if let Some(text) = for_label_map.get(id.as_str()) {
            if !text.is_empty() {
                return Some(text.clone());
            }
        }
    }

    // 3b. Wrapping <label>
    if let Some(text) = wrapped_label_map.get(&node.node_id) {
        if !text.is_empty() {
            return Some(text.clone());
        }
    }

    // 4. Element-specific native semantics
    match node.tag.as_str() {
        // Images: alt attribute
        "img" | "area" => {
            if let Some(alt) = node.attributes.get("alt") {
                return Some(alt.clone());
            }
        }
        // <figure>: name from <figcaption> child
        "figure" => {
            if let Some(cap) = node.children.iter().find(|c| c.tag == "figcaption") {
                let text = collect_text(cap);
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
        // <table>: name from <caption> child
        "table" => {
            if let Some(cap) = node
                .children
                .iter()
                .flat_map(|c| std::iter::once(c).chain(c.children.iter()))
                .find(|c| c.tag == "caption")
            {
                let text = collect_text(cap);
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
        // <fieldset>: name from <legend> child
        "fieldset" => {
            if let Some(leg) = node.children.iter().find(|c| c.tag == "legend") {
                let text = collect_text(leg);
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
        _ => {}
    }

    // 5. Name from text content — only for roles that derive their name from content.
    //    Replaced/form elements (input, textarea, select) don't have meaningful
    //    text children; container-named elements already handled above.
    let has_name_from_content = !matches!(
        node.tag.as_str(),
        "input" | "textarea" | "select" | "figure" | "table" | "fieldset"
    );
    if has_name_from_content {
        let text = collect_text(node);
        if !text.is_empty() {
            return Some(text);
        }
    }

    // 6. placeholder — fallback name for text inputs with no other label
    if matches!(node.tag.as_str(), "input" | "textarea") {
        if let Some(ph) = node.attributes.get("placeholder") {
            if !ph.is_empty() {
                return Some(ph.clone());
            }
        }
    }

    // 7. title as last resort
    node.attributes
        .get("title")
        .filter(|s| !s.is_empty())
        .cloned()
}

/// Recursively collect visible text content of a subtree.
#[cfg(feature = "accessibility")]
fn collect_text(node: &WebCore) -> String {
    let mut s = node.text.trim().to_string();
    for child in &node.children {
        if !matches!(child.style.display, Display::None) && child.style.visibility {
            let ct = collect_text(child);
            if !ct.is_empty() {
                if !s.is_empty() {
                    s.push(' ');
                }
                s.push_str(&ct);
            }
        }
    }
    s
}

/// Returns true if this element participates in the keyboard tab order.
#[cfg(feature = "accessibility")]
fn is_focusable(node: &WebCore) -> bool {
    let tag = node.tag.as_str();
    matches!(tag, "button" | "input" | "textarea" | "select")
        || (tag == "a" && node.attributes.contains_key("href"))
        || node
            .attributes
            .get("tabindex")
            .and_then(|v| v.parse::<i32>().ok())
            .map(|n| n >= 0)
            .unwrap_or(false)
        || node
            .attributes
            .get("contenteditable")
            .map(|v| v == "true" || v.is_empty())
            .unwrap_or(false)
}
