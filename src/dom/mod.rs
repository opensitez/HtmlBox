//! DOM manipulation, events API, and editing utilities.
//!
//! This module ports the C++ `wxHtmlEditWidget` DOM/events/editing API to Rust.

use std::time::{Duration, Instant};
use std::sync::{Arc, RwLock};
use crate::types::{HtmlBox, Document, Color, Display, FontWeight, FontStyle, CssLength};
use crate::css::apply_property;
use crate::layout::hit_test::point_to_hit;

const CARET_BLINK_MS: u64 = 500;

// ─── Event types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtmlEventType {
    // Mouse
    Click, DblClick, MouseDown, MouseUp, MouseMove,
    MouseEnter, MouseLeave, ContextMenu,
    // Drag
    DragStart, Drag, DragEnter, DragOver, DragLeave, Drop, DragEnd,
    // Keyboard
    KeyDown, KeyUp, KeyPress,
    // Content/focus/scroll
    Scroll, Input, Change, Focus, Blur,
    SelectionChange,
}

// ─── HtmlEvent ────────────────────────────────────────────────────────────────

pub struct HtmlEvent {
    pub event_type:  HtmlEventType,
    /// Deepest box hit (valid only during dispatch).
    pub target:          *const HtmlBox,
    /// Box the current listener is registered on.
    pub current_target:  *const HtmlBox,
    /// Root of the document tree (valid only during dispatch).
    pub root:        *const HtmlBox,
    /// Position in window coordinates.
    pub client_pos:  (f32, f32),
    /// Position in document coordinates.
    pub doc_pos:     (f32, f32),
    /// Mouse button: 0=left, 1=middle, 2=right.
    pub button:      u8,
    pub key_code:    u32,
    pub char_code:   Option<char>,
    pub ctrl_key:    bool,
    pub shift_key:   bool,
    pub alt_key:     bool,
    pub meta_key:    bool,
    /// Source box for drag events.
    pub drag_source:     *const HtmlBox,
    pub related_target:  *const HtmlBox,
    pub default_prevented:   bool,
    pub propagation_stopped: bool,
}

impl HtmlEvent {
    pub fn new(event_type: HtmlEventType) -> Self {
        Self {
            event_type,
            target:          std::ptr::null(),
            current_target:  std::ptr::null(),
            root:            std::ptr::null(),
            client_pos:      (0.0, 0.0),
            doc_pos:         (0.0, 0.0),
            button:          0,
            key_code:        0,
            char_code:       None,
            ctrl_key:        false,
            shift_key:       false,
            alt_key:         false,
            meta_key:        false,
            drag_source:     std::ptr::null(),
            related_target:  std::ptr::null(),
            default_prevented:   false,
            propagation_stopped: false,
        }
    }

    pub fn stop_propagation(&mut self) { self.propagation_stopped = true; }
    pub fn prevent_default(&mut self) { self.default_prevented = true; }
}

// ─── Event listener registry ──────────────────────────────────────────────────

pub type HtmlEventCallback = Box<dyn Fn(&mut HtmlEvent) + Send + Sync + 'static>;

struct EventListenerEntry {
    id:         i32,
    selector:   String,
    event_type: HtmlEventType,
    callback:   HtmlEventCallback,
}

struct EventListenersInner {
    entries: Vec<EventListenerEntry>,
    next_id: i32,
}

/// Holds all registered event listeners.  Store one in your application state.
#[derive(Clone, Default)]
pub struct EventListeners {
    inner: Arc<RwLock<EventListenersInner>>,
}

impl Default for EventListenersInner {
    fn default() -> Self {
        Self { entries: Vec::new(), next_id: 1 }
    }
}

impl std::fmt::Debug for EventListeners {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.read().unwrap();
        f.debug_struct("EventListeners")
            .field("entry_count", &inner.entries.len())
            .field("next_id", &inner.next_id)
            .finish()
    }
}

impl EventListeners {
    pub fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(EventListenersInner::default())) }
    }

    /// Register a listener.  Returns an ID that can be used to remove it later.
    pub fn add(
        &self,
        selector:   impl Into<String>,
        event_type: HtmlEventType,
        callback:   HtmlEventCallback,
    ) -> i32 {
        let mut inner = self.inner.write().unwrap();
        let id = inner.next_id;
        inner.next_id += 1;
        inner.entries.push(EventListenerEntry {
            id,
            selector: selector.into(),
            event_type,
            callback,
        });
        id
    }

    pub fn remove(&self, id: i32) {
        self.inner.write().unwrap().entries.retain(|e| e.id != id);
    }

    pub fn remove_by_selector(&self, selector: &str) {
        self.inner.write().unwrap().entries.retain(|e| e.selector != selector);
    }

    pub fn remove_by_selector_and_type(&self, selector: &str, t: HtmlEventType) {
        self.inner.write().unwrap().entries.retain(|e| !(e.selector == selector && e.event_type == t));
    }

    pub fn remove_all(&self) {
        self.inner.write().unwrap().entries.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.inner.read().unwrap().entries.is_empty()
    }

    /// Dispatch an event from a hit-test target, bubbling up through ancestors.
    /// Returns `true` if any handler was executed (useful for triggered redraws).
    pub fn dispatch(&self, root: &HtmlBox, mut evt: HtmlEvent) -> bool {
        let inner = self.inner.read().unwrap();
        if inner.entries.is_empty() { return false; }
        evt.root = root as *const HtmlBox;

        let mut handled = false;
        let target = evt.target;

        if !target.is_null() {
            // Build the ancestor path [root … parent, target]
            let path = find_ancestor_path(root, target);

            // Bubble: fire from target outward
            for &box_ptr in path.iter().rev() {
                let b = unsafe { &*box_ptr };
                evt.current_target = box_ptr;
                for entry in &inner.entries {
                    if entry.event_type != evt.event_type { continue; }
                    if !matches_simple_selector(b, &entry.selector) { continue; }
                    (entry.callback)(&mut evt);
                    handled = true;
                    if evt.propagation_stopped { break; }
                }
                if evt.propagation_stopped { break; }
            }
        } else {
            // No positional target: fire on root (keyboard, scroll, selection-change)
            evt.current_target = root as *const HtmlBox;
            for entry in &inner.entries {
                if entry.event_type != evt.event_type { continue; }
                let sel = entry.selector.as_str();
                if sel != "*" && sel != "html" && sel != "body" { continue; }
                (entry.callback)(&mut evt);
                handled = true;
                if evt.propagation_stopped { break; }
            }
        }

        handled || evt.default_prevented
    }
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Build path `[root, ..., parent, target]` from root down to `target`.
fn find_ancestor_path(root: &HtmlBox, target: *const HtmlBox) -> Vec<*const HtmlBox> {
    let mut path = Vec::new();
    collect_path(root, target, &mut path);
    path
}

fn collect_path(
    node:   &HtmlBox,
    target: *const HtmlBox,
    path:   &mut Vec<*const HtmlBox>,
) -> bool {
    path.push(node as *const HtmlBox);
    if std::ptr::eq(node as *const HtmlBox, target) { return true; }
    for child in &node.children {
        if collect_path(child, target, path) { return true; }
    }
    path.pop();
    false
}

/// Simple CSS selector matching: `tag`, `#id`, `.class`, `*`.
pub fn matches_simple_selector(b: &HtmlBox, selector: &str) -> bool {
    if selector == "*" { return true; }
    if let Some(id_sel) = selector.strip_prefix('#') {
        return b.attributes.get("id").map(|s| s == id_sel).unwrap_or(false);
    }
    if let Some(cls_sel) = selector.strip_prefix('.') {
        return b.attributes.get("class")
            .map(|s| s.split_whitespace().any(|c| c == cls_sel))
            .unwrap_or(false);
    }
    b.tag == selector
}

// ─── DOM class manipulation ───────────────────────────────────────────────────

/// Add a CSS class to a box.
pub fn add_class(b: &mut HtmlBox, cls: &str) {
    if has_class(b, cls) { return; }
    let entry = b.attributes.entry("class".to_string()).or_default();
    if entry.is_empty() {
        *entry = cls.to_string();
    } else {
        entry.push(' ');
        entry.push_str(cls);
    }
}

/// Remove a CSS class from a box.
pub fn remove_class(b: &mut HtmlBox, cls: &str) {
    if let Some(val) = b.attributes.get_mut("class") {
        let new: Vec<&str> = val.split_whitespace().filter(|&c| c != cls).collect();
        *val = new.join(" ");
    }
}

/// Toggle a CSS class on a box.
pub fn toggle_class(b: &mut HtmlBox, cls: &str) {
    if has_class(b, cls) { remove_class(b, cls); } else { add_class(b, cls); }
}

/// Returns true if the box has the given CSS class.
pub fn has_class(b: &HtmlBox, cls: &str) -> bool {
    b.attributes.get("class")
        .map(|s| s.split_whitespace().any(|c| c == cls))
        .unwrap_or(false)
}

// ─── DOM attribute manipulation ───────────────────────────────────────────────

/// Set an attribute on a box.  Handles `id`, `class`, `style`, `href` specially.
pub fn set_attribute(b: &mut HtmlBox, attr: &str, value: &str) {
    match attr {
        "id"    => { b.attributes.insert("id".to_string(), value.to_string()); }
        "class" => { b.attributes.insert("class".to_string(), value.to_string()); }
        "style" => { apply_inline_style_str(b, value); }
        _       => { b.attributes.insert(attr.to_string(), value.to_string()); }
    }
}

/// Get an attribute from a box.  Returns `None` if not present.
pub fn get_attribute<'a>(b: &'a HtmlBox, attr: &str) -> Option<&'a str> {
    match attr {
        "tag"   => Some(b.tag.as_str()),
        _       => b.attributes.get(attr).map(|s| s.as_str()),
    }
}

/// Remove an attribute from a box.
pub fn remove_attribute(b: &mut HtmlBox, attr: &str) {
    match attr {
        "id"    => { b.attributes.remove("id"); }
        "class" => { b.attributes.remove("class"); }
        _       => { b.attributes.remove(attr); }
    }
}

// ─── Custom data ─────────────────────────────────────────────────────────────

pub fn set_data(b: &mut HtmlBox, key: &str, value: &str) {
    b.data.insert(key.to_string(), value.to_string());
}

pub fn get_data<'a>(b: &'a HtmlBox, key: &str) -> Option<&'a str> {
    b.data.get(key).map(|s| s.as_str())
}

pub fn has_data(b: &HtmlBox, key: &str) -> bool {
    b.data.contains_key(key)
}

pub fn remove_data(b: &mut HtmlBox, key: &str) {
    b.data.remove(key);
}

// ─── Visibility ───────────────────────────────────────────────────────────────

/// Hide a box (sets `display: none`).
pub fn hide(b: &mut HtmlBox) {
    b.style.display = Display::None;
}

/// Show a box (restores block display if hidden).
pub fn show(b: &mut HtmlBox) {
    if b.style.display == Display::None {
        b.style.display = Display::Block;
    }
}

pub fn is_visible(b: &HtmlBox) -> bool {
    b.style.display != Display::None
}

// ─── Inline style property ───────────────────────────────────────────────────

/// Apply a single CSS property to a box's computed style.
pub fn set_style_property(b: &mut HtmlBox, prop: &str, value: &str) {
    apply_property(&mut b.style, prop, value);
}

/// Apply a `key: val; key: val` style string to a box.
pub fn apply_inline_style_str(b: &mut HtmlBox, css: &str) {
    for decl in css.split(';') {
        let decl = decl.trim();
        if decl.is_empty() { continue; }
        if let Some(colon) = decl.find(':') {
            let prop = decl[..colon].trim();
            let val  = decl[colon+1..].trim();
            if !prop.is_empty() && !val.is_empty() {
                apply_property(&mut b.style, prop, val);
            }
        }
    }
}

// ─── Query selector ───────────────────────────────────────────────────────────

/// Returns the first box matching the selector, searching depth-first.
pub fn query_selector<'a>(root: &'a HtmlBox, selector: &str) -> Option<&'a HtmlBox> {
    if matches_simple_selector(root, selector) { return Some(root); }
    for child in &root.children {
        if let Some(found) = query_selector(child, selector) {
            return Some(found);
        }
    }
    None
}

/// Mutable version of `query_selector`.
pub fn query_selector_mut<'a>(root: &'a mut HtmlBox, selector: &str) -> Option<&'a mut HtmlBox> {
    if matches_simple_selector(root, selector) { return Some(root); }
    for child in &mut root.children {
        if let Some(found) = query_selector_mut(child, selector) {
            return Some(found);
        }
    }
    None
}

/// Returns all boxes matching the selector.
pub fn query_selector_all<'a>(root: &'a HtmlBox, selector: &str) -> Vec<&'a HtmlBox> {
    let mut out = Vec::new();
    collect_all(root, selector, &mut out);
    out
}

fn collect_all<'a>(node: &'a HtmlBox, sel: &str, out: &mut Vec<&'a HtmlBox>) {
    if matches_simple_selector(node, sel) { out.push(node); }
    for child in &node.children { collect_all(child, sel, out); }
}

/// Returns all boxes matching the selector (mutable).
pub fn query_selector_all_mut<'a>(root: &'a mut HtmlBox, selector: &str) -> Vec<&'a mut HtmlBox> {
    let mut out: Vec<*mut HtmlBox> = Vec::new();
    collect_all_mut_internal(root as *mut HtmlBox, selector, &mut out);
    out.into_iter().map(|p| unsafe { &mut *p }).collect()
}

fn collect_all_mut_internal(node: *mut HtmlBox, sel: &str, out: &mut Vec<*mut HtmlBox>) {
    unsafe {
        if matches_simple_selector(&*node, sel) { out.push(node); }
        for child in &mut (*node).children {
            collect_all_mut_internal(child as *mut HtmlBox, sel, out);
        }
    }
}

// ─── Tree traversal ───────────────────────────────────────────────────────────

pub fn get_first_child(b: &HtmlBox) -> Option<&HtmlBox> {
    b.children.first()
}

pub fn get_last_child(b: &HtmlBox) -> Option<&HtmlBox> {
    b.children.last()
}

/// Find the next sibling of `target` within `parent`.
pub fn get_next_sibling<'a>(parent: &'a HtmlBox, target: *const HtmlBox) -> Option<&'a HtmlBox> {
    let idx = parent.children.iter()
        .position(|c| std::ptr::eq(c as *const HtmlBox, target))?;
    parent.children.get(idx + 1)
}

/// Find the previous sibling of `target` within `parent`.
pub fn get_prev_sibling<'a>(parent: &'a HtmlBox, target: *const HtmlBox) -> Option<&'a HtmlBox> {
    let idx = parent.children.iter()
        .position(|c| std::ptr::eq(c as *const HtmlBox, target))?;
    if idx == 0 { return None; }
    parent.children.get(idx - 1)
}

// ─── DOM tree mutation ────────────────────────────────────────────────────────

/// Append `child` as the last child of `parent`.
pub fn append_child(parent: &mut HtmlBox, child: HtmlBox) {
    parent.children.push(child);
}

/// Prepend `child` as the first child of `parent`.
pub fn prepend_child(parent: &mut HtmlBox, child: HtmlBox) {
    parent.children.insert(0, child);
}

/// Insert `new_node` before `reference` within `parent`.
/// Returns `false` if `reference` was not found.
pub fn insert_before(parent: &mut HtmlBox, reference: *const HtmlBox, new_node: HtmlBox) -> bool {
    if let Some(idx) = parent.children.iter()
        .position(|c| std::ptr::eq(c as *const HtmlBox, reference))
    {
        parent.children.insert(idx, new_node);
        true
    } else {
        false
    }
}

/// Insert `new_node` after `reference` within `parent`.
pub fn insert_after(parent: &mut HtmlBox, reference: *const HtmlBox, new_node: HtmlBox) -> bool {
    if let Some(idx) = parent.children.iter()
        .position(|c| std::ptr::eq(c as *const HtmlBox, reference))
    {
        parent.children.insert(idx + 1, new_node);
        true
    } else {
        false
    }
}

/// Remove the child at position `index` from `parent`, returning it.
pub fn remove_child_at(parent: &mut HtmlBox, index: usize) -> Option<HtmlBox> {
    if index < parent.children.len() {
        Some(parent.children.remove(index))
    } else {
        None
    }
}

/// Remove the child identified by raw pointer from `parent`, returning it.
pub fn remove_child(parent: &mut HtmlBox, target: *const HtmlBox) -> Option<HtmlBox> {
    if let Some(idx) = parent.children.iter()
        .position(|c| std::ptr::eq(c as *const HtmlBox, target))
    {
        Some(parent.children.remove(idx))
    } else {
        None
    }
}

/// Deep-clone an element (HtmlBox implements Clone).
pub fn clone_element(b: &HtmlBox) -> HtmlBox {
    b.clone()
}

/// Create a new element with the given tag name.
pub fn create_element(tag: &str) -> HtmlBox {
    HtmlBox::new(tag)
}

// ─── Text content ─────────────────────────────────────────────────────────────

/// Get the concatenated text content of a box and all descendants.
pub fn get_text_content(b: &HtmlBox) -> String {
    b.text_content()
}

/// Replace all children with a single text node and set `b.text`.
pub fn set_text_content(b: &mut HtmlBox, text: &str) {
    b.children.clear();
    b.inline_runs.clear();
    b.text = text.to_string();
}

// ─── Editing: toggle formatting on selection ──────────────────────────────────

/// Describes a text range within a single `HtmlBox` (local byte offsets).
pub struct TextRange {
    pub start: usize,
    pub end:   usize,
}

/// Toggle `font-weight: bold` on the inline runs that overlap `range`.
pub fn toggle_bold(b: &mut HtmlBox, range: &TextRange) {
    let was_bold = b.inline_runs.iter()
        .filter(|r| r.text_offset < range.end && r.text_offset + r.length > range.start)
        .all(|r| r.style.font_weight.is_bold());

    for run in &mut b.inline_runs {
        if run.text_offset < range.end && run.text_offset + run.length > range.start {
            run.style.font_weight = if was_bold { FontWeight::Normal } else { FontWeight::Bold };
        }
    }
}

/// Toggle `font-style: italic` on the inline runs that overlap `range`.
pub fn toggle_italic(b: &mut HtmlBox, range: &TextRange) {
    let was_italic = b.inline_runs.iter()
        .filter(|r| r.text_offset < range.end && r.text_offset + r.length > range.start)
        .all(|r| r.style.font_style == FontStyle::Italic);

    for run in &mut b.inline_runs {
        if run.text_offset < range.end && run.text_offset + run.length > range.start {
            run.style.font_style =
                if was_italic { FontStyle::Normal } else { FontStyle::Italic };
        }
    }
}

/// Toggle `text-decoration: underline` on overlapping runs.
pub fn toggle_underline(b: &mut HtmlBox, range: &TextRange) {
    let was_underline = b.inline_runs.iter()
        .filter(|r| r.text_offset < range.end && r.text_offset + r.length > range.start)
        .all(|r| r.style.text_decoration.underline);

    for run in &mut b.inline_runs {
        if run.text_offset < range.end && run.text_offset + run.length > range.start {
            run.style.text_decoration.underline = !was_underline;
        }
    }
}

/// Toggle `text-decoration: line-through` on overlapping runs.
pub fn toggle_strikethrough(b: &mut HtmlBox, range: &TextRange) {
    let was = b.inline_runs.iter()
        .filter(|r| r.text_offset < range.end && r.text_offset + r.length > range.start)
        .all(|r| r.style.text_decoration.strikethrough);

    for run in &mut b.inline_runs {
        if run.text_offset < range.end && run.text_offset + run.length > range.start {
            run.style.text_decoration.strikethrough = !was;
        }
    }
}

/// Set font size (in px) on overlapping runs.
pub fn set_font_size(b: &mut HtmlBox, range: &TextRange, size_px: f32) {
    for run in &mut b.inline_runs {
        if run.text_offset < range.end && run.text_offset + run.length > range.start {
            run.style.font_size = CssLength::Px(size_px);
        }
    }
}

/// Set font family on overlapping runs.
pub fn set_font_family(b: &mut HtmlBox, range: &TextRange, family: &str) {
    for run in &mut b.inline_runs {
        if run.text_offset < range.end && run.text_offset + run.length > range.start {
            run.style.font_family = family.to_string();
        }
    }
}

/// Set text color on overlapping runs.
pub fn set_text_color(b: &mut HtmlBox, range: &TextRange, color: Color) {
    for run in &mut b.inline_runs {
        if run.text_offset < range.end && run.text_offset + run.length > range.start {
            run.style.color = color;
        }
    }
}

/// Set background color on overlapping runs.
pub fn set_bg_color(b: &mut HtmlBox, range: &TextRange, color: Color) {
    for run in &mut b.inline_runs {
        if run.text_offset < range.end && run.text_offset + run.length > range.start {
            run.style.background_color = color;
        }
    }
}

// ─── Undo/Redo ────────────────────────────────────────────────────────────────

/// A snapshot for undo/redo (clones the whole document tree).
pub struct UndoEntry {
    /// Serialized snapshot (caret/selection positions baked in as metadata).
    pub doc:        Document,
    pub caret_pos:  usize,
    pub sel_start:  usize,
    pub sel_end:    usize,
}

/// Simple undo/redo stack (max 500 entries).
pub struct UndoStack {
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
}

impl Default for UndoStack {
    fn default() -> Self { Self::new() }
}

impl UndoStack {
    pub fn new() -> Self { Self { undo: Vec::new(), redo: Vec::new() } }

    /// Snapshot the current document state before a mutation.
    pub fn push(&mut self, doc: Document, caret_pos: usize, sel_start: usize, sel_end: usize) {
        self.undo.push(UndoEntry { doc, caret_pos, sel_start, sel_end });
        if self.undo.len() > 500 { self.undo.remove(0); }
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool { !self.undo.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo.is_empty() }

    /// Undo: moves current doc → redo, returns previous entry.
    pub fn undo(
        &mut self,
        current_doc: Document,
        current_caret: usize,
        current_sel_start: usize,
        current_sel_end: usize,
    ) -> Option<UndoEntry> {
        let entry = self.undo.pop()?;
        self.redo.push(UndoEntry {
            doc: current_doc,
            caret_pos:  current_caret,
            sel_start:  current_sel_start,
            sel_end:    current_sel_end,
        });
        Some(entry)
    }

    /// Redo: moves current doc → undo, returns next entry.
    pub fn redo(
        &mut self,
        current_doc: Document,
        current_caret: usize,
        current_sel_start: usize,
        current_sel_end: usize,
    ) -> Option<UndoEntry> {
        let entry = self.redo.pop()?;
        self.undo.push(UndoEntry {
            doc: current_doc,
            caret_pos:  current_caret,
            sel_start:  current_sel_start,
            sel_end:    current_sel_end,
        });
        Some(entry)
    }
}

// ─── Editor ──────────────────────────────────────────────────────────────────

/// High-level editor state and operations.
#[derive(Debug, Clone)]
pub struct Editor {
    pub caret_box:    Option<*const HtmlBox>,
    pub caret_local:  usize,
    pub sel_anchor:   usize,
    pub sel_start:    usize,
    pub sel_end:      usize,
    pub caret_visible: bool,
    pub last_blink:   Instant,
    pub mouse_down:   bool,
    pub has_focus:    bool,
    pub read_only:    bool,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            caret_box:    None,
            caret_local:  0,
            sel_anchor:   0,
            sel_start:    0,
            sel_end:      0,
            caret_visible: true,
            last_blink:   Instant::now(),
            mouse_down:   false,
            has_focus:    false,
            read_only:    false,
        }
    }
}

impl Editor {
    pub fn new() -> Self { Self::default() }

    pub fn has_selection(&self) -> bool { self.sel_start < self.sel_end }

    pub fn caret_info(&self) -> Option<(*const HtmlBox, usize)> {
        self.caret_box.map(|p| (p, self.caret_local))
    }

    pub fn sel_args(&self) -> (Option<usize>, Option<usize>) {
        if self.has_selection() {
            (Some(self.sel_start), Some(self.sel_end))
        } else {
            (None, None)
        }
    }

    pub fn set_caret_from_hit(&mut self, box_ptr: *const HtmlBox, local: usize, extend: bool) {
        if !extend {
            self.sel_anchor  = local;
            self.sel_start   = local;
            self.sel_end     = local;
        }
        self.caret_box   = Some(box_ptr);
        self.caret_local = local;
        if extend && self.caret_box == Some(box_ptr) {
            self.sel_start = self.sel_anchor.min(local);
            self.sel_end   = self.sel_anchor.max(local);
        }
        self.caret_visible = true;
        self.last_blink    = Instant::now();
    }

    pub fn move_left(&mut self, flat: &str, extend: bool) {
        let pos = self.caret_local;
        if !extend && self.has_selection() {
            let new = self.sel_start;
            self.collapse_to(new);
            return;
        }
        let new = prev_char_boundary(flat, pos);
        self.move_to(new, extend);
    }

    pub fn move_right(&mut self, flat: &str, extend: bool) {
        let pos = self.caret_local;
        if !extend && self.has_selection() {
            let new = self.sel_end;
            self.collapse_to(new);
            return;
        }
        let new = next_char_boundary(flat, pos);
        self.move_to(new, extend);
    }

    pub fn move_to(&mut self, new_pos: usize, extend: bool) {
        self.caret_local = new_pos;
        if !extend {
            self.sel_anchor = new_pos;
            self.sel_start  = new_pos;
            self.sel_end    = new_pos;
        } else {
            self.sel_start = self.sel_anchor.min(new_pos);
            self.sel_end   = self.sel_anchor.max(new_pos);
        }
        self.caret_visible = true;
        self.last_blink    = Instant::now();
    }

    pub fn collapse_to(&mut self, pos: usize) {
        self.caret_local = pos;
        self.sel_anchor  = pos;
        self.sel_start   = pos;
        self.sel_end     = pos;
        self.caret_visible = true;
        self.last_blink    = Instant::now();
    }

    pub fn blink_update(&mut self) -> bool {
        if self.last_blink.elapsed() >= Duration::from_millis(CARET_BLINK_MS) {
            self.caret_visible = !self.caret_visible;
            self.last_blink    = Instant::now();
            true
        } else {
            false
        }
    }

    pub fn next_blink_deadline(&self) -> Instant {
        self.last_blink + Duration::from_millis(CARET_BLINK_MS)
    }

    /// Primary mouse event handler for selection/caret.
    pub fn handle_mouse_event(&mut self, root: &HtmlBox, etype: HtmlEventType, doc_pt: (f32, f32)) -> bool {
        match etype {
            HtmlEventType::MouseDown => {
                self.mouse_down = true;
                self.has_focus  = true;
                if let Some(hit) = point_to_hit(root, doc_pt) {
                    self.set_caret_from_hit(hit.box_ptr, hit.local_offset, false);
                    return true;
                }
            }
            HtmlEventType::MouseMove => {
                if self.mouse_down {
                    if let Some(hit) = point_to_hit(root, doc_pt) {
                        if self.caret_box == Some(hit.box_ptr) {
                            self.caret_local = hit.local_offset;
                            self.sel_start = self.sel_anchor.min(hit.local_offset);
                            self.sel_end   = self.sel_anchor.max(hit.local_offset);
                            self.caret_visible = true;
                            return true;
                        }
                    }
                }
            }
            HtmlEventType::MouseUp => {
                self.mouse_down = false;
            }
            _ => {}
        }
        false
    }

    /// Primary keyboard event handler. Returns true if redraw needed.
    pub fn handle_key_event(&mut self, root: &mut HtmlBox, etype: HtmlEventType, key_code: u32, ch: Option<char>, _ctrl: bool) -> bool {
        if self.read_only { return false; }
        if etype != HtmlEventType::KeyDown && etype != HtmlEventType::KeyPress { return false; }

        let box_ptr = match self.caret_box { Some(p) => p, None => return false };

        match key_code {
            37 => { // ArrowLeft
                let flat = crate::layout::inline_layout::collect_flat_text(unsafe { &*box_ptr });
                self.move_left(&flat, false);
                return true;
            }
            39 => { // ArrowRight
                let flat = crate::layout::inline_layout::collect_flat_text(unsafe { &*box_ptr });
                self.move_right(&flat, false);
                return true;
            }
            8 => { // Backspace
                self.delete_selection_or_before(root);
                return true;
            }
            46 => { // Delete
                self.delete_selection_or_at(root);
                return true;
            }
            13 => { // Enter
                self.insert_char(root, '\n');
                return true;
            }
            _ => {
                if let Some(c) = ch {
                    if !c.is_control() {
                        self.insert_char(root, c);
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn insert_char(&mut self, root: &mut HtmlBox, ch: char) {
        let box_ptr = match self.caret_box { Some(p) => p, None => return };
        if let Some(container) = find_box_mut(root, box_ptr) {
            if self.has_selection() {
                let s = self.sel_start;
                let e = self.sel_end;
                delete_range(container, s, e);
                self.collapse_to(s);
            }
            match find_node_offset_mut(container, self.caret_local) {
                Ok((leaf, local)) => {
                    let mut buf = [0u8; 4];
                    let s = ch.encode_utf8(&mut buf);
                    leaf.text.insert_str(local, s);
                    self.caret_local += s.len();
                    self.collapse_to(self.caret_local);
                }
                Err(_) => {
                    let ins = self.caret_local.min(container.text.len());
                    container.text.insert(ins, ch);
                    self.caret_local += ch.len_utf8();
                    self.collapse_to(self.caret_local);
                }
            }
        }
    }

    pub fn delete_selection_or_before(&mut self, root: &mut HtmlBox) {
        let box_ptr = match self.caret_box { Some(p) => p, None => return };
        if let Some(node) = find_box_mut(root, box_ptr) {
            if self.has_selection() {
                let s = self.sel_start;
                let e = self.sel_end;
                delete_range(node, s, e);
                self.collapse_to(s);
            } else {
                if self.caret_local > 0 {
                    let flat = crate::layout::inline_layout::collect_flat_text(node);
                    let new_off = prev_char_boundary(&flat, self.caret_local);
                    delete_range(node, new_off, self.caret_local);
                    self.collapse_to(new_off);
                }
            }
        }
    }

    pub fn delete_selection_or_at(&mut self, root: &mut HtmlBox) {
        let box_ptr = match self.caret_box { Some(p) => p, None => return };
        if let Some(node) = find_box_mut(root, box_ptr) {
            if self.has_selection() {
                let s = self.sel_start;
                let e = self.sel_end;
                delete_range(node, s, e);
                self.collapse_to(s);
            } else {
                let flat = crate::layout::inline_layout::collect_flat_text(node);
                if self.caret_local < flat.len() {
                    let next_off = next_char_boundary(&flat, self.caret_local);
                    delete_range(node, self.caret_local, next_off);
                    self.collapse_to(self.caret_local);
                }
            }
        }
    }
}

// ─── Internal Editor helpers ──────────────────────────────────────────────────

fn prev_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx == 0 { return 0; }
    idx -= 1;
    while idx > 0 && !s.is_char_boundary(idx) { idx -= 1; }
    idx
}

fn next_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() { return s.len(); }
    let mut i = idx + 1;
    while i < s.len() && !s.is_char_boundary(i) { i += 1; }
    i
}


pub fn find_box_mut<'a>(root: &'a mut HtmlBox, ptr: *const HtmlBox) -> Option<&'a mut HtmlBox> {
    if std::ptr::eq(root as *const HtmlBox, ptr) { return Some(root); }
    for child in &mut root.children {
        if let Some(b) = find_box_mut(child, ptr) { return Some(b); }
    }
    None
}

/// Resolves a global offset (from collect_flat_text) to a specific leaf node and local offset.
fn find_node_offset_mut(node: &mut HtmlBox, mut offset: usize) -> Result<(&mut HtmlBox, usize), usize> {
    if node.is_text_node() {
        if offset <= node.text.len() {
            return Ok((node, offset));
        } else {
            return Err(offset - node.text.len());
        }
    }
    if matches!(node.style.display, Display::None) { return Err(offset); }
    if node.tag == "br" { return Err(offset); }

    if !node.text.is_empty() {
        if offset <= node.text.len() {
            return Ok((node, offset));
        } else {
            offset -= node.text.len();
        }
    }

    for child in &mut node.children {
        match find_node_offset_mut(child, offset) {
            Ok(res) => return Ok(res),
            Err(rem) => offset = rem,
        }
    }
    Err(offset)
}

fn delete_range(node: &mut HtmlBox, start: usize, end: usize) -> (usize, usize) {
    // This is a simplified range deletion that only works within a single node for now.
    // In a real editor, this needs to handle multi-node spans.
    if let Ok((leaf, local_s)) = find_node_offset_mut(node, start) {
        let len = (end - start).min(leaf.text.len() - local_s);
        leaf.text.drain(local_s..local_s + len);
        return (len, 0); // (deleted_count, remaining)
    }
    (0, end - start)
}
