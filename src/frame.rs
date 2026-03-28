//! Frame loop — the engine's self-contained update cycle.
//!
//! `EngineFrame` is the primary API for both **browser mode** and **app engine mode**.
//! The host should never need to call cascade/layout/paint manually — just feed
//! content and events, and the engine handles everything internally.
//!
//! ## Browser mode
//! ```ignore
//! let mut frame = EngineFrame::new(800.0, 600.0);
//! frame.load_html("<h1>Hello</h1>");
//! // ... on vsync:
//! if frame.update_frame() {
//!     renderer.render(&frame.doc, &mut pixmap, scale);
//! }
//! ```
//!
//! ## App engine mode
//! ```ignore
//! let mut frame = EngineFrame::new(800.0, 600.0);
//! frame.load_html("<div id='root'></div>");
//! let root = frame.query_selector("#root").unwrap();
//! frame.set_inner_html(root, "<button>Click me</button>");
//! // ... on vsync:
//! if frame.update_frame() {
//!     renderer.render(&frame.doc, &mut pixmap, scale);
//! }
//! ```
//!
//! ## Event handling
//! ```ignore
//! frame.mouse_move(x, y);       // hover tracking
//! frame.mouse_event(Click, pt, 0); // click
//! frame.scroll(0.0, -30.0);     // scroll
//! frame.resize(1024.0, 768.0);  // viewport change
//! frame.key_input("a");         // text input
//! ```
//!
//! The host only needs to:
//! 1. Create an EngineFrame
//! 2. Set content (load_html, DOM API, or navigate)
//! 3. Forward input events
//! 4. Call update_frame() on vsync — it returns true when pixels changed
//! 5. Paint using renderer.render() when update_frame() returns true

use crate::types::Document;
use crate::layout::LayoutEngine;

/// Callbacks the engine fires to notify the host of state changes.
/// The host implements this trait — the engine calls it, never the other way around.
///
/// All methods have default no-op implementations so the host only needs to
/// override what it cares about.
pub trait EngineCallbacks {
    /// First paint is ready (above-fold content laid out and display list built).
    fn on_first_paint(&mut self) {}
    /// Document fully loaded and laid out (all resources fetched).
    fn on_load_complete(&mut self) {}
    /// Document scroll extent changed — update scrollbar.
    fn on_scroll_height_changed(&mut self, _height: f32) {}
    /// Title changed (`<title>` element parsed or updated).
    fn on_title_changed(&mut self, _title: &str) {}
    /// Navigation requested (link clicked, form submitted).
    fn on_navigate(&mut self, _url: &str) {}
    /// Layout completed — for benchmarking / profiling.
    fn on_layout_complete(&mut self, _duration_ms: f32) {}
    /// Cursor style should change (pointer, text, default, etc.).
    fn on_cursor_changed(&mut self, _cursor: crate::types::CSSCursor) {}
}

/// No-op callbacks — used when host doesn't register any.
struct NoopCallbacks;
impl EngineCallbacks for NoopCallbacks {}

/// The self-contained engine. Wraps Document + LayoutEngine into a frame-based
/// update cycle. The host feeds content and events; the engine handles
/// cascade, layout, and display list internally.
pub struct EngineFrame {
    pub doc: Document,
    pub engine: LayoutEngine,
    /// Whether any DOM mutation or style change occurred since last frame.
    needs_style: bool,
    /// Whether layout needs to run (set when style changes geometry-affecting properties).
    needs_layout: bool,
    /// Whether the display needs redrawing (set after any visible change).
    needs_paint: bool,
    /// Viewport dimensions.
    viewport_w: f32,
    viewport_h: f32,
    /// Last reported scroll height — used to detect changes for callback.
    last_scroll_height: f32,
    /// Whether initial content has been loaded (for on_first_paint).
    first_paint_done: bool,
    /// Host callbacks (boxed trait object).
    callbacks: Box<dyn EngineCallbacks>,
}

impl EngineFrame {
    /// Create from an existing Document.
    pub fn new(doc: Document, viewport_w: f32, viewport_h: f32) -> Self {
        let mut engine = LayoutEngine::new();
        engine.viewport_w = viewport_w;
        engine.viewport_h = viewport_h;
        Self {
            doc,
            engine,
            needs_style: true,
            needs_layout: true,
            needs_paint: true,
            viewport_w,
            viewport_h,
            last_scroll_height: 0.0,
            first_paint_done: false,
            callbacks: Box::new(NoopCallbacks),
        }
    }

    /// Create an empty engine — content is set via `load_html()` or DOM API.
    pub fn empty(viewport_w: f32, viewport_h: f32) -> Self {
        let doc = crate::html::parse_html("<html><head></head><body></body></html>");
        Self::new(doc, viewport_w, viewport_h)
    }

    /// Register host callbacks. The engine pushes events — the host never polls.
    pub fn set_callbacks(&mut self, callbacks: impl EngineCallbacks + 'static) {
        self.callbacks = Box::new(callbacks);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Content loading — the host just says "here's HTML" or "navigate to URL"
    // ═══════════════════════════════════════════════════════════════════════════

    /// Load HTML content. Parses, fetches external CSS, cascades, and lays out.
    /// This is the primary way to set content in browser mode.
    pub fn load_html(&mut self, html: &str) {
        self.load_html_with_base(html, "");
    }

    /// Load HTML with a base URL for resolving relative links and resources.
    pub fn load_html_with_base(&mut self, html: &str, base_url: &str) {
        self.doc = crate::load_html_with_registry(
            html, base_url, self.viewport_w, self.viewport_h,
            self.engine.component_registry.clone(),
        );
        self.first_paint_done = false;
        self.needs_style = true;
        self.needs_layout = true;
        self.needs_paint = true;
        self.engine.invalidate_cascade();
    }

    /// Set the full body HTML content (keeps existing <head> stylesheets).
    /// Lighter than load_html for app-style content updates.
    pub fn set_body_html(&mut self, html: &str) {
        if let Some(body_id) = self.doc.query_selector("body") {
            self.doc.dom_set_inner_html(body_id, html);
            self.mark_style_dirty();
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Frame update — the core of the engine
    // ═══════════════════════════════════════════════════════════════════════════

    /// Run one frame of the engine update cycle.
    /// Returns `true` if the screen needs redrawing.
    ///
    /// Call this on every vsync (60fps), or after any event/mutation.
    /// It does the **minimum work needed**:
    /// - Nothing changed → returns false immediately (0ms).
    /// - Only hover changed → incremental cascade + layout on ~10 nodes.
    /// - DOM was mutated → cascade + layout on dirty subtrees.
    /// - Viewport resized → full cascade + layout.
    /// - Only scrolled → returns true (repaint only, no layout).
    pub fn update_frame(&mut self) -> bool {
        // 1. Poll for async images/fonts
        if self.doc.poll_pending_images() {
            self.needs_layout = true;
            self.needs_paint = true;
        }
        self.engine.poll_pending_fonts();

        // 2. Check if hover changed (set by process_mouse_event)
        if self.doc.hover_changed {
            self.needs_style = true;
            self.needs_layout = true;
            self.needs_paint = true;
        }

        // 3. Check for running animations
        if self.doc.needs_animation_frame {
            self.needs_style = true;
            self.needs_layout = true;
            self.needs_paint = true;
        }

        // 4. Style + Layout (batched — all mutations since last frame processed at once)
        if self.needs_style || self.needs_layout {
            let t0 = std::time::Instant::now();
            if self.needs_style {
                self.engine.layout(&mut self.doc, self.viewport_w);
            } else {
                self.engine.layout_no_cascade(&mut self.doc, self.viewport_w);
            }
            self.needs_style = false;
            self.needs_layout = false;
            self.needs_paint = true;

            // Notify host of layout completion
            let duration_ms = t0.elapsed().as_secs_f32() * 1000.0;
            self.callbacks.on_layout_complete(duration_ms);

            // Check if scroll height changed — notify host for scrollbar update
            let sh = crate::types::Document::scroll_height(&self.doc.root)
                .max(self.doc.root.layout.margin_rect.h);
            if (sh - self.last_scroll_height).abs() > 1.0 {
                self.last_scroll_height = sh;
                self.callbacks.on_scroll_height_changed(sh);
            }

            // First paint callback
            if !self.first_paint_done {
                self.first_paint_done = true;
                self.callbacks.on_first_paint();
            }
        }

        // 5. Paint flag
        if self.needs_paint {
            self.needs_paint = false;
            return true;
        }

        false
    }

    /// Check if the engine needs a repaint without consuming the flag.
    pub fn needs_render(&self) -> bool {
        self.needs_paint || self.needs_style || self.needs_layout
            || self.doc.hover_changed || self.doc.needs_animation_frame
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Input events — host forwards raw events, engine handles everything
    // ═══════════════════════════════════════════════════════════════════════════

    /// Set the viewport size. Triggers re-cascade + re-layout on next frame.
    pub fn set_viewport(&mut self, w: f32, h: f32) {
        if (w - self.viewport_w).abs() > 0.5 || (h - self.viewport_h).abs() > 0.5 {
            self.viewport_w = w;
            self.viewport_h = h;
            self.engine.viewport_w = w;
            self.engine.viewport_h = h;
            self.needs_style = true;  // media queries may change
            self.needs_layout = true;
            self.needs_paint = true;
        }
    }

    /// Alias for set_viewport.
    pub fn resize(&mut self, w: f32, h: f32) {
        self.set_viewport(w, h);
    }

    /// Scroll by delta. No layout needed — just repaint with new offset.
    pub fn scroll(&mut self, dx: f32, dy: f32) {
        let doc_h = crate::types::Document::scroll_height(&self.doc.root)
            .max(self.doc.root.layout.margin_rect.h);
        let doc_w = self.doc.root.layout.margin_rect.w;
        let view_h = self.viewport_h;
        let view_w = self.viewport_w;

        let new_y = (self.doc.scroll_y + dy).max(0.0).min((doc_h - view_h).max(0.0));
        let new_x = (self.doc.scroll_x + dx).max(0.0).min((doc_w - view_w).max(0.0));

        if (new_y - self.doc.scroll_y).abs() > 0.01 || (new_x - self.doc.scroll_x).abs() > 0.01 {
            self.doc.scroll_y = new_y;
            self.doc.scroll_x = new_x;
            self.needs_paint = true; // repaint only, no layout
        }
    }

    /// Set scroll position absolutely.
    pub fn scroll_to(&mut self, x: f32, y: f32) {
        let doc_h = crate::types::Document::scroll_height(&self.doc.root)
            .max(self.doc.root.layout.margin_rect.h);
        let doc_w = self.doc.root.layout.margin_rect.w;
        let new_y = y.max(0.0).min((doc_h - self.viewport_h).max(0.0));
        let new_x = x.max(0.0).min((doc_w - self.viewport_w).max(0.0));
        if (new_y - self.doc.scroll_y).abs() > 0.01 || (new_x - self.doc.scroll_x).abs() > 0.01 {
            self.doc.scroll_y = new_y;
            self.doc.scroll_x = new_x;
            self.needs_paint = true;
        }
    }

    /// Mouse moved to document-space coordinates. Handles hover tracking.
    pub fn mouse_move(&mut self, doc_x: f32, doc_y: f32) {
        let new_hovered = crate::layout::hit_test::hit_test_box_at(
            &self.doc.root, (doc_x, doc_y), 0,
        );
        if new_hovered != self.doc.hovered_box {
            self.doc.hovered_box = new_hovered;
            self.doc.hover_changed = true;
        }
    }

    /// Process a mouse event (click, mousedown, mouseup) and mark dirty if needed.
    pub fn mouse_event(&mut self, etype: crate::dom::HtmlEventType, doc_pt: (f32, f32), button: u8) -> bool {
        let redraw = self.doc.process_mouse_event(etype, doc_pt, button);
        if redraw {
            self.needs_paint = true;
            if self.doc.hover_changed {
                self.needs_style = true;
                self.needs_layout = true;
            }
        }
        redraw
    }

    /// Get the total scroll height of the document.
    pub fn scroll_height(&self) -> f32 {
        crate::types::Document::scroll_height(&self.doc.root)
            .max(self.doc.root.layout.margin_rect.h)
    }

    /// Get the current scroll position.
    pub fn scroll_position(&self) -> (f32, f32) {
        (self.doc.scroll_x, self.doc.scroll_y)
    }

    /// Get the viewport dimensions.
    pub fn viewport(&self) -> (f32, f32) {
        (self.viewport_w, self.viewport_h)
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Dirty tracking — mark what changed
    // ═══════════════════════════════════════════════════════════════════════════

    /// Mark that the DOM or styles have changed and need re-cascade.
    pub fn mark_style_dirty(&mut self) {
        self.doc.style_dirty = true;
        self.needs_style = true;
        self.needs_layout = true;
        self.needs_paint = true;
    }

    /// Mark that layout needs to run (e.g. after text content change).
    pub fn mark_layout_dirty(&mut self) {
        self.needs_layout = true;
        self.needs_paint = true;
    }

    /// Mark that the display needs redrawing (e.g. after cursor blink).
    pub fn mark_paint_dirty(&mut self) {
        self.needs_paint = true;
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // DOM API — mutations are queued, layout runs on next update_frame()
    // ═══════════════════════════════════════════════════════════════════════════

    /// Get the root element's node ID (the <html> element).
    pub fn root_id(&self) -> u32 {
        self.doc.root.node_id
    }

    /// Query for an element by CSS selector. Returns node ID if found.
    pub fn query_selector(&self, selector: &str) -> Option<u32> {
        self.doc.query_selector(selector)
    }

    /// Create an element and mark style dirty.
    pub fn create_element(&mut self, tag: &str) -> u32 {
        let id = self.doc.dom_create_element(tag);
        self.mark_style_dirty();
        id
    }

    /// Create a text node and mark style dirty.
    pub fn create_text(&mut self, text: &str) -> u32 {
        let id = self.doc.dom_create_text(text);
        self.mark_style_dirty();
        id
    }

    /// Append child and mark dirty.
    pub fn append_child(&mut self, parent: u32, child: u32) {
        self.doc.dom_append_child(parent, child);
        self.mark_style_dirty();
    }

    /// Remove child and mark dirty.
    pub fn remove_child(&mut self, child: u32) {
        self.doc.dom_remove_child(child);
        self.mark_style_dirty();
    }

    /// Set attribute and mark dirty.
    pub fn set_attribute(&mut self, id: u32, key: &str, val: &str) {
        self.doc.dom_set_attribute(id, key, val);
        self.mark_style_dirty();
    }

    /// Toggle class and mark dirty.
    pub fn toggle_class(&mut self, id: u32, class: &str) -> bool {
        let result = self.doc.class_list_toggle(id, class);
        self.mark_style_dirty();
        result
    }

    /// Add a CSS class.
    pub fn add_class(&mut self, id: u32, class: &str) {
        if let Some(node) = self.doc.get_box_by_id_mut(id) {
            crate::dom::add_class(node, class);
            self.mark_style_dirty();
        }
    }

    /// Remove a CSS class.
    pub fn remove_class(&mut self, id: u32, class: &str) {
        if let Some(node) = self.doc.get_box_by_id_mut(id) {
            crate::dom::remove_class(node, class);
            self.mark_style_dirty();
        }
    }

    /// Set inline style property and mark dirty.
    pub fn set_style(&mut self, id: u32, prop: &str, val: &str) {
        self.doc.set_style_property(id, prop, val);
        self.mark_style_dirty();
    }

    /// Set text content and mark dirty.
    pub fn set_text_content(&mut self, id: u32, text: &str) {
        self.doc.dom_set_text_content(id, text);
        self.mark_style_dirty();
    }

    /// Set inner HTML and mark dirty.
    pub fn set_inner_html(&mut self, id: u32, html: &str) {
        self.doc.dom_set_inner_html(id, html);
        self.mark_style_dirty();
    }

    /// Add a CSS stylesheet (like injecting a <style> tag).
    pub fn add_stylesheet(&mut self, css: &str) {
        self.doc.stylesheet.parse_and_add(css);
        self.mark_style_dirty();
        self.engine.invalidate_cascade();
    }

    /// Set a CSS variable on the root element.
    pub fn set_css_var(&mut self, name: &str, value: &str) {
        self.doc.root.style.custom_props.insert(name.to_string(), value.to_string());
        self.mark_style_dirty();
        self.engine.invalidate_cascade();
    }

    /// Apply a theme (set of CSS variables) on :root.
    pub fn set_theme(&mut self, vars: &[(&str, &str)]) {
        for &(name, value) in vars {
            self.doc.root.style.custom_props.insert(name.to_string(), value.to_string());
        }
        self.mark_style_dirty();
        self.engine.invalidate_cascade();
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Event API — host registers callbacks, engine dispatches
    // ═══════════════════════════════════════════════════════════════════════════

    /// Register an event listener on a node. Returns listener ID.
    pub fn on(&mut self, node_id: u32, event_type: &str,
              handler: crate::dom::events::EventHandler) -> u32 {
        self.doc.event_targets.add_event_listener(node_id, event_type, handler, false)
    }

    /// Register a capture-phase event listener. Returns listener ID.
    pub fn on_capture(&mut self, node_id: u32, event_type: &str,
                      handler: crate::dom::events::EventHandler) -> u32 {
        self.doc.event_targets.add_event_listener(node_id, event_type, handler, true)
    }

    /// Remove an event listener by ID.
    pub fn off(&mut self, listener_id: u32) {
        self.doc.event_targets.remove_event_listener(listener_id);
    }

    /// Dispatch a DOM event through capture → target → bubble.
    pub fn dispatch_event(&self, event: &mut crate::dom::events::DomEvent) -> bool {
        self.doc.event_targets.dispatch_event(&self.doc.arena, event)
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Query API — read computed state
    // ═══════════════════════════════════════════════════════════════════════════

    /// Get the computed bounding rectangle of an element (document coordinates).
    pub fn get_bounding_rect(&self, id: u32) -> Option<crate::types::Rect> {
        self.doc.get_node(id).map(|n| n.layout.border_rect)
    }

    /// Get the text content of an element.
    pub fn get_text_content(&self, id: u32) -> Option<String> {
        self.doc.get_node(id).map(|n| crate::dom::get_text_content(n))
    }

    /// Get an attribute value.
    pub fn get_attribute(&self, id: u32, key: &str) -> Option<String> {
        self.doc.get_node(id).and_then(|n| n.attributes.get(key).cloned())
    }
}
