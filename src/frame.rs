//! Frame loop — the engine's update cycle.
//!
//! `EngineFrame` encapsulates the Document + LayoutEngine and provides
//! `update_frame()` which batches all pending changes and runs the minimum
//! amount of work (style, layout, paint) per frame.
//!
//! Usage:
//! ```ignore
//! let mut frame = EngineFrame::new(doc, 800.0, 600.0);
//! // ... modify DOM via frame.doc ...
//! if frame.update_frame() {
//!     renderer.render(&frame.doc, &mut pixmap);
//! }
//! ```

use crate::types::Document;
use crate::layout::LayoutEngine;

/// Wraps Document + LayoutEngine into a frame-based update cycle.
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
}

impl EngineFrame {
    pub fn new(doc: Document, viewport_w: f32, viewport_h: f32) -> Self {
        let mut engine = LayoutEngine::new();
        engine.viewport_w = viewport_w;
        engine.viewport_h = viewport_h;
        Self {
            doc,
            engine,
            needs_style: true,   // initial cascade needed
            needs_layout: true,  // initial layout needed
            needs_paint: true,
            viewport_w,
            viewport_h,
        }
    }

    /// Set the viewport size. Triggers re-layout on next frame.
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

    /// Run one frame of the engine update cycle.
    /// Returns `true` if the screen needs redrawing.
    ///
    /// This is the core of the engine — call it on every vsync (60fps),
    /// mouse move, or after DOM mutations. It does the minimum work needed:
    /// - If nothing changed, returns false immediately.
    /// - If only hover changed, runs incremental cascade + layout on ~10 nodes.
    /// - If DOM was mutated, runs cascade + layout on dirty subtrees.
    /// - If viewport resized, runs full cascade + layout.
    pub fn update_frame(&mut self) -> bool {
        // 1. Poll for async images
        if self.doc.poll_pending_images() {
            self.needs_layout = true;
            self.needs_paint = true;
        }

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

        // 4. Style + Layout (layout() handles cascade internally,
        //    including incremental cascade for hover-only changes)
        if self.needs_style || self.needs_layout {
            if self.needs_style {
                self.engine.layout(&mut self.doc, self.viewport_w);
            } else {
                self.engine.layout_no_cascade(&mut self.doc, self.viewport_w);
            }
            self.needs_style = false;
            self.needs_layout = false;
            self.needs_paint = true;
        }

        // 5. Paint flag
        if self.needs_paint {
            self.needs_paint = false;
            return true;
        }

        false
    }

    // ── Convenience DOM API wrappers that auto-mark dirty ──

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

    // ── Event API wrappers ──

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

    /// Process a mouse event and mark dirty if needed.
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
}
