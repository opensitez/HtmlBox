use tiny_skia::{
    FillRule, LineCap, Mask, Paint, PathBuilder, Pixmap, Rect as SkRect, Stroke, Transform,
};
use cosmic_text::{
    Attrs, Buffer, Color as CTextColor, FontSystem, Metrics, Shaping, SwashCache,
    Style as CTextStyle, Weight,
};
use crate::layout::inline_layout::{css_family_to_cosmic, stretch_from_percent, weight_from_style};
use winit::event::{TouchPhase, WindowEvent};
use winit::keyboard::Key;
use crate::types::*;
use crate::layout::inline_layout::collect_flat_text;

const SCROLLBAR_WIDTH: f32 = 10.0;

// ─── Renderer ─────────────────────────────────────────────────────────────────

pub struct Renderer {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub component_registry: ComponentRegistry,
    /// Zoom level: 1.0 = 100%, 2.0 = 200%, 0.5 = 50%.
    /// Updated automatically by `handle_window_event` (pinch, Ctrl+Wheel, Ctrl+=/−/0).
    /// Can also be set directly by the host.
    pub zoom: f32,
    scale: f32,
    /// Reused across draw_text_run calls to avoid per-chunk Buffer allocation.
    shape_buf: Option<Buffer>,
    // ── Internal state for handle_window_event ────────────────────────────────
    ctrl_held:       bool,
    shift_held:      bool,
    touches:         std::collections::HashMap<u64, (f64, f64)>,
    pinch_dist:      Option<f32>,
    touch_centroid:  Option<(f32, f32)>,
    /// Last known cursor position in physical pixels (for hover hit-testing).
    cursor_physical: (f32, f32),
    /// Logical viewport height (layout pixels) — kept in sync by `render()` so
    /// that `vh` units and `flex-stretch` heights work on every repaint.
    viewport_h: f32,
    /// Element IDs (HtmlBox pointer as usize) that currently have active transitions
    /// or animation overrides. When set, the renderer uses node.style (already has
    /// the interpolated values applied) rather than hover_style for these elements.
    transitioning_ids: std::collections::HashSet<usize>,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            component_registry: ComponentRegistry::default(),
            zoom: 1.0,
            scale: 1.0,
            shape_buf: None,
            ctrl_held:       false,
            shift_held:      false,
            touches:         std::collections::HashMap::new(),
            pinch_dist:      None,
            touch_centroid:  None,
            cursor_physical: (0.0, 0.0),
            viewport_h: 700.0,
            transitioning_ids: std::collections::HashSet::new(),
        }
    }

    /// Handle a winit `WindowEvent` for built-in zoom and pan.
    ///
    /// Call this from your event loop **before** your own event handling.
    /// Pass `doc` so scroll positions can be updated directly.
    /// Returns `true` if zoom or scroll changed and a redraw should be requested.
    ///
    /// Events consumed (caller should skip its own handling when true is returned):
    /// - `PinchGesture`        — trackpad pinch zoom (macOS / iOS)
    /// - `PanGesture`          — two-finger trackpad pan, all directions (macOS)
    /// - `Touch`               — two-finger pinch+pan on touchscreens (all platforms)
    /// - `ModifiersChanged`    — internal Ctrl tracking (always returns false)
    /// - `MouseWheel`+Ctrl     — zoom in/out
    /// - `MouseWheel` plain    — scroll x+y; returns false so the app can also
    ///                           call `process_wheel_event` for inner-box scrolling
    /// - `KeyboardInput`+Ctrl  — `=`/`+` zoom in, `-` zoom out, `0` reset
    pub fn handle_window_event(&mut self, event: &WindowEvent, mut doc: Option<&mut crate::types::Document>) -> bool {
        match event {
            WindowEvent::ModifiersChanged(mods) => {
                self.ctrl_held  = mods.state().control_key();
                self.shift_held = mods.state().shift_key();
                false
            }

            // ── macOS/iOS trackpad pinch ──────────────────────────────────────
            WindowEvent::PinchGesture { delta, .. } => {
                self.zoom = (self.zoom * (1.0 + *delta as f32)).clamp(0.1, 8.0);
                true
            }

            // ── macOS two-finger pan (all directions) ─────────────────────────
            WindowEvent::PanGesture { delta, .. } => {
                if let Some(doc) = doc {
                    let zoom = self.zoom;
                    doc.scroll_x = (doc.scroll_x - delta.x / zoom).max(0.0);
                    doc.scroll_y = (doc.scroll_y - delta.y / zoom).max(0.0);
                }
                true
            }

            // ── Touchscreen two-finger pinch + pan ───────────────────────────
            WindowEvent::Touch(winit::event::Touch { phase, location, id, .. }) => {
                match phase {
                    TouchPhase::Started => {
                        self.touches.insert(*id, (location.x, location.y));
                        // Reset gesture state on any new finger.
                        if self.touches.len() < 2 {
                            self.pinch_dist     = None;
                            self.touch_centroid = None;
                        }
                        false
                    }
                    TouchPhase::Moved => {
                        self.touches.insert(*id, (location.x, location.y));
                        if self.touches.len() == 2 {
                            let pts: Vec<(f64, f64)> = self.touches.values().copied().collect();
                            // Centroid for pan.
                            let cx = ((pts[0].0 + pts[1].0) / 2.0) as f32;
                            let cy = ((pts[0].1 + pts[1].1) / 2.0) as f32;
                            // Distance for pinch zoom.
                            let dx = pts[0].0 - pts[1].0;
                            let dy = pts[0].1 - pts[1].1;
                            let new_dist = ((dx * dx + dy * dy) as f32).sqrt();

                            if let Some(prev_dist) = self.pinch_dist {
                                if prev_dist > 1.0 {
                                    self.zoom = (self.zoom * new_dist / prev_dist).clamp(0.1, 8.0);
                                }
                            }
                            if let (Some((px, py)), Some(doc)) = (self.touch_centroid, doc.as_deref_mut()) {
                                // Physical px delta → divide by DPI scale and zoom.
                                let sc   = self.scale.max(1.0);
                                let zoom = self.zoom;
                                doc.scroll_x = (doc.scroll_x - (cx - px) / sc / zoom).max(0.0);
                                doc.scroll_y -= (cy - py) / sc / zoom;
                            }
                            self.pinch_dist     = Some(new_dist);
                            self.touch_centroid = Some((cx, cy));
                            true
                        } else { false }
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        self.touches.remove(id);
                        if self.touches.len() < 2 {
                            self.pinch_dist     = None;
                            self.touch_centroid = None;
                        }
                        false
                    }
                }
            }

            // ── Ctrl+Wheel → zoom ─────────────────────────────────────────────
            WindowEvent::MouseWheel { delta, .. } if self.ctrl_held => {
                let dy = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => *y,
                    winit::event::MouseScrollDelta::PixelDelta(p)   => p.y as f32 / 20.0,
                };
                self.zoom = (self.zoom * 1.1f32.powf(dy)).clamp(0.1, 8.0);
                true
            }

            // ── Plain wheel → dispatch Wheel event then scroll ───────────────
            WindowEvent::MouseWheel { delta, .. } => {
                let sc = self.scale.max(1.0);
                let (dx, dy) = match delta {
                    // LineDelta: positive y = scroll up → negate for browser convention.
                    winit::event::MouseScrollDelta::LineDelta(x, y) => (*x * 20.0, -*y * 20.0),
                    winit::event::MouseScrollDelta::PixelDelta(p)   =>
                        (p.x as f32 / sc, -(p.y as f32 / sc)),
                };

                if let Some(doc) = doc {
                    let client_x = self.cursor_physical.0 / sc;
                    let client_y = self.cursor_physical.1 / sc;
                    // doc_pt accounts for scroll_y (vertical viewport scroll) and
                    // scroll_x for horizontal panning.
                    let doc_pt = (
                        client_x / self.zoom + doc.scroll_x,
                        client_y / self.zoom + doc.scroll_y,
                    );
                    let mut evt = crate::dom::HtmlEvent::new(crate::dom::HtmlEventType::Wheel);
                    evt.client_pos = (client_x, client_y);
                    evt.doc_pos    = doc_pt;
                    evt.delta_x    = dx;
                    evt.delta_y    = dy;
                    evt.target = crate::layout::hit_test::point_to_hit(&doc.root, doc_pt, 0)
                        .map(|h| h.box_ptr)
                        .unwrap_or(std::ptr::null());
                    let events = doc.events.clone();
                    events.dispatch(&doc.root, evt);
                    // dx/dy are in browser-event convention (positive = scroll right/down).
                    // process_wheel_event uses the opposite convention (negative = scroll down)
                    // inherited from main.rs LineDelta usage, so negate before forwarding.
                    return doc.process_wheel_event_xy(doc_pt, -dx, -dy);
                }
                false
            }

            // ── Ctrl+=/+/−/0 keyboard shortcuts ──────────────────────────────
            WindowEvent::KeyboardInput { event, .. }
                if self.ctrl_held && event.state == winit::event::ElementState::Pressed =>
            {
                match &event.logical_key {
                    Key::Character(s) if s == "=" || s == "+" => {
                        self.zoom = (self.zoom * 1.2).clamp(0.1, 8.0); true
                    }
                    Key::Character(s) if s == "-" => {
                        self.zoom = (self.zoom / 1.2).clamp(0.1, 8.0); true
                    }
                    Key::Character(s) if s == "0" => {
                        self.zoom = 1.0; true
                    }
                    _ => false,
                }
            }

            // ── CursorMoved → update hover + fire MouseMove / MouseOver / MouseOut / Pointer ──
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_physical = (position.x as f32, position.y as f32);
                if let Some(doc) = doc {
                    let sc   = self.scale.max(1.0);
                    let zoom = self.zoom;
                    let sx = self.cursor_physical.0 / sc;
                    let sy = self.cursor_physical.1 / sc;
                    let pt = (sx / zoom, sy / zoom + doc.scroll_y);
                    let mut redraw = doc.process_mouse_event(crate::dom::HtmlEventType::MouseMove, pt, 0);
                    // PointerMove mirrors MouseMove
                    redraw |= doc.process_mouse_event(crate::dom::HtmlEventType::PointerMove, pt, 0);
                    // MouseOver / MouseOut fire when the hit target changes (bubbling Enter/Leave)
                    redraw |= doc.dispatch_over_out(pt);
                    return redraw;
                }
                false
            }

            // ── MouseInput → active state, MouseDown/Up, Pointer mirror ──────
            WindowEvent::MouseInput { state, button, .. } => {
                let bt = match button {
                    winit::event::MouseButton::Left   => 0u8,
                    winit::event::MouseButton::Middle => 1,
                    winit::event::MouseButton::Right  => 2,
                    _ => 0,
                };
                if let Some(doc) = doc {
                    let sc   = self.scale.max(1.0);
                    let zoom = self.zoom;
                    let sx = self.cursor_physical.0 / sc;
                    let sy = self.cursor_physical.1 / sc;
                    let pt = (sx / zoom, sy / zoom + doc.scroll_y);
                    let (mouse_type, ptr_type) = if *state == winit::event::ElementState::Pressed {
                        (crate::dom::HtmlEventType::MouseDown, crate::dom::HtmlEventType::PointerDown)
                    } else {
                        (crate::dom::HtmlEventType::MouseUp, crate::dom::HtmlEventType::PointerUp)
                    };
                    let mut redraw = doc.process_mouse_event(mouse_type, pt, bt);
                    redraw |= doc.process_mouse_event(ptr_type, pt, bt);
                    return redraw;
                }
                false
            }

            // ── Resize → fire Resize event on document root ───────────────────
            WindowEvent::Resized(size) => {
                if let Some(doc) = doc {
                    let mut evt = crate::dom::HtmlEvent::new(crate::dom::HtmlEventType::Resize);
                    evt.client_pos = (size.width as f32, size.height as f32);
                    let events = doc.events.clone();
                    events.dispatch(&doc.root, evt);
                }
                false // host still needs to call platform.resize() / re-layout
            }

            // ── Tab / Shift+Tab → keyboard focus navigation ───────────────────
            WindowEvent::KeyboardInput { event, .. }
                if event.state == winit::event::ElementState::Pressed =>
            {
                if let Key::Named(winit::keyboard::NamedKey::Tab) = &event.logical_key {
                    if let Some(doc) = doc {
                        return if self.shift_held {
                            doc.focus_prev()
                        } else {
                            doc.focus_next()
                        };
                    }
                }
                false
            }

            _ => false,
        }
    }

    pub fn register_component(&mut self, tag: &str, measure: ComponentMeasureFn, paint: ComponentPaintFn) {
        self.component_registry.register(tag, measure, paint);
    }

    /// Set the DPI / device-pixel scale that will be used for layout and rendering.
    ///
    /// Call this with `platform.scale_factor()` **before** the first
    /// `layout_engine().layout()` call so that `fill_char_x_for_line` shapes text
    /// at the same physical-pixel size as `draw_text_run`.  Without this, on HiDPI
    /// (Retina) displays the two shape at different sizes, font hinting produces
    /// different advances, and click-to-caret mapping is off by several characters.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    /// Create a [`LayoutEngine`] that shares this renderer's font system so that
    /// line-breaking metrics match the actual rendered glyph widths.
    pub fn layout_engine(&mut self) -> crate::layout::LayoutEngine {
        let mut engine = crate::layout::LayoutEngine::new();
        engine.font_system = Some(&mut self.font_system as *mut _);
        engine.component_registry = self.component_registry.clone();
        engine.viewport_h = self.viewport_h;
        engine.scale = self.scale;
        engine
    }

    /// Parse HTML and run layout using the renderer's font system, so that text
    /// measurement during layout matches the actual rendered glyph widths.
    /// Prefer this over the top-level `load_html` when rendering with this renderer.
    pub fn load_html(&mut self, html: &str, viewport_width: f32) -> crate::Document {
        self.load_html_vp(html, viewport_width, 700.0)
    }

    /// Like [`load_html`] but with explicit viewport height (for `vh`/`100vh` layouts).
    pub fn load_html_vp(&mut self, html: &str, viewport_width: f32, viewport_height: f32) -> crate::Document {
        self.load_html_with_base(html, "", viewport_width, viewport_height)
    }

    /// Like [`load_html_vp`] but with a base URL for resolving external resources.
    pub fn load_html_with_base(&mut self, html: &str, base_url: &str, viewport_width: f32, viewport_height: f32) -> crate::Document {
        let mut doc = crate::load_html_with_base(html, base_url, viewport_width, viewport_height);
        // Re-layout with the renderer's font metrics so glyph widths match rendering.
        let mut engine = self.layout_engine();
        engine.viewport_h = viewport_height;
        engine.layout(&mut doc, viewport_width);
        doc
    }

    /// Render the full document onto a pixmap.
    ///
    /// `scale` — HiDPI scale factor (physical pixels / logical pixel); pass the value
    /// provided by `Platform::render`.
    ///
    /// `caret_info` — `Some((box_ptr, local_byte_offset))` where `box_ptr` is a raw pointer
    /// to the `HtmlBox` that owns the caret and `local_byte_offset` is the byte index within
    /// that box's flat text.  Mirrors C++ `Render(... caretPos, caretVisible, hasFocus)`.
    /// Render one frame.
    /// Hover/active state is updated automatically by `handle_window_event`
    /// on `CursorMoved` and `MouseInput` events — no extra work needed here.
    pub fn render(
        &mut self,
        doc:    &mut Document,
        pixmap: &mut Pixmap,
        scale:  f32,
    ) {
        let (sel_start, sel_end) = doc.editor.sel_args();
        let caret_info = doc.editor.caret_info();
        let caret_visible = doc.editor.caret_visible;
        let _has_focus = doc.editor.has_focus;
        let sel_box_ptr: *const HtmlBox = caret_info
            .map(|(ptr, _)| ptr)
            .unwrap_or(std::ptr::null());
        self.scale = scale;
        let zoom = self.zoom.clamp(0.1, 8.0);
        // CSS canvas background: use the body element's background if set,
        // otherwise fall back to the root (html) element's background, then white.
        let canvas_color = doc.root.children.iter()
            .find(|c| c.tag == "body")
            .map(|body| body.style.background_color)
            .filter(|c| c.a > 0)
            .or_else(|| {
                let c = doc.root.style.background_color;
                if c.a > 0 { Some(c) } else { None }
            })
            .map(|c| c.to_tiny_skia())
            .unwrap_or(tiny_skia::Color::WHITE);
        pixmap.fill(canvas_color);
        // Logical viewport dimensions (physical pixels / DPI scale).
        let w = pixmap.width()  as f32 / self.scale;
        let h = pixmap.height() as f32 / self.scale;
        // Keep viewport_h in sync with the actual window height so that vh units
        // and flex-stretch heights are correct on the next layout call.
        let view_h = h / self.zoom.clamp(0.1, 8.0);
        self.viewport_h = view_h;
        // Visible portion of the document (in layout/logical coordinates).
        // At zoom=1 this equals the full viewport; at zoom=2 it's half as large.
        let view_w = w / zoom;
        let view_h = h / zoom;
        // Culling clip uses the visible document area, not the full viewport.
        let clip = Rect::new(0.0, 0.0, view_w, view_h);

        // Clamp scroll so the document never scrolls past its own end; write back.
        // When <html>'s bottom margin collapses with <body>'s (via CSS margin-collapse),
        // doc.root.margin_rect.h can be shorter than body's actual content bottom by the
        // amount of the collapsed margin.  Use the body's padding-box bottom if it is larger.
        let doc_h = {
            let root_h = doc.root.margin_rect.h;
            doc.root.children.iter()
                .find(|c| c.tag == "body")
                .map(|b| root_h.max(b.padding_rect.y + b.padding_rect.h))
                .unwrap_or(root_h)
        };
        let doc_w = doc.root.margin_rect.w;
        doc.scroll_y = doc.scroll_y.max(0.0).min((doc_h - view_h).max(0.0));
        doc.scroll_x = doc.scroll_x.max(0.0).min((doc_w - view_w).max(0.0));
        let scroll_x = doc.scroll_x;
        let scroll_y = doc.scroll_y;

        // Bake zoom into self.scale for all content drawing this frame.
        // draw_text_run shapes text at font_px * self.scale physical pixels, so
        // including zoom here gives sharper glyph rasterization at higher zoom levels.
        // Restored to DPI-only scale after content is drawn (before scrollbar).
        self.scale = scale * zoom;

        let hovered_ptr   = doc.hovered_box;
        let active_ptr    = doc.active_box;
        let visited_hrefs = &doc.visited_urls;
        // Collect element IDs that have active transitions so render_box can
        // use node.style (already has interpolated overrides) instead of hover_style.
        self.transitioning_ids = doc.animation_overrides.keys().cloned().collect();
        self.render_box(
            &doc.root, pixmap,
            scroll_x, scroll_y,
            &clip,
            /* parent_mask */ None,
            sel_start, sel_end,
            sel_box_ptr,
            hovered_ptr,
            active_ptr,
            visited_hrefs,
        );

        // ── Caret ─────────────────────────────────────────────────────────────
        // Only draw the caret when the caret box lives inside a contenteditable
        // element — never in read-only document content.
        if caret_visible {
            if let Some((caret_box_ptr, caret_local)) = caret_info {
                let in_editable = crate::dom::is_in_contenteditable(&doc.root, caret_box_ptr);
                if in_editable {
                    self.draw_caret(
                        &doc.root, pixmap,
                        scroll_x, scroll_y,
                        caret_box_ptr, caret_local,
                    );
                }
            }
        }

        // Restore DPI-only scale: the scrollbar is viewport UI, not document content.
        self.scale = scale;

        // ── Viewport scrollbar (auto — visible whenever content overflows) ────
        if doc_h > view_h {
            let thumb_col = doc.root.style.scrollbar_thumb_color
                .unwrap_or(Color::rgba(128, 128, 128, 160));
            let track_col = doc.root.style.scrollbar_track_color
                .unwrap_or(Color::rgba(128, 128, 128, 40));
            let track_h = h;
            let thumb_h = (track_h * view_h / doc_h).max(20.0);
            let max_s   = doc_h - view_h;
            let thumb_y = if max_s > 0.0 { scroll_y * (track_h - thumb_h) / max_s } else { 0.0 };
            let track_x = w - SCROLLBAR_WIDTH;
            let ts = Transform::from_scale(self.scale, self.scale);
            let mut paint = Paint::default();
            paint.set_color(track_col.to_tiny_skia());
            if let Some(r) = SkRect::from_xywh(track_x, 0.0, SCROLLBAR_WIDTH, track_h) {
                pixmap.fill_rect(r, &paint, ts, None);
            }
            paint.set_color(thumb_col.to_tiny_skia());
            if let Some(path) = rounded_rect_path(track_x + 1.0, thumb_y + 1.0,
                    SCROLLBAR_WIDTH - 2.0, thumb_h - 2.0, 3.0) {
                pixmap.fill_path(&path, &paint, FillRule::Winding, ts, None);
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Main per-box render  (mirrors RenderBox in C++)
    // ─────────────────────────────────────────────────────────────────────────

    fn render_box(
        &mut self,
        node:          &HtmlBox,
        pixmap:        &mut Pixmap,
        scroll_x:      f32,
        scroll_y:      f32,
        clip:          &Rect,
        parent_mask:   Option<&Mask>,
        sel_start:     Option<usize>,
        sel_end:       Option<usize>,
        sel_box_ptr:   *const HtmlBox,
        hovered_ptr:   *const HtmlBox,
        active_ptr:    *const HtmlBox,
        visited_hrefs: &std::collections::HashSet<String>,
    ) {
        if matches!(node.style.display, Display::None) { return; }
        if !node.style.visibility { return; }
        if node.style.opacity <= 0.0 { return; }

        let sx = scroll_x;
        let sy = scroll_y;
        let br = node.border_rect;

        // ── Viewport culling ──────────────────────────────────────────────────
        if matches!(node.style.position, Position::Static | Position::Relative) {
            let bx = br.x - sx;
            let by = br.y - sy;
            if bx + br.w < clip.x || by + br.h < clip.y
                || bx > clip.right() || by > clip.bottom()
            {
                return;
            }
        }

        let pr      = node.padding_rect;
        let px      = pr.x - sx;
        let py      = pr.y - sy;
        let pw      = pr.w;
        let ph      = pr.h;
        let font_px = node.style.font_size_px(16.0, 16.0);
        let r_shorthand = node.style.border_radius.resolve(font_px, pr.w, 16.0);
        let r_tl = if r_shorthand > 0.0 { r_shorthand }
            else { node.style.border_top_left_radius.resolve(font_px, pr.w, 16.0) };
        let r_tr = if r_shorthand > 0.0 { r_shorthand }
            else { node.style.border_top_right_radius.resolve(font_px, pr.w, 16.0) };
        let r_br = if r_shorthand > 0.0 { r_shorthand }
            else { node.style.border_bottom_right_radius.resolve(font_px, pr.w, 16.0) };
        let r_bl = if r_shorthand > 0.0 { r_shorthand }
            else { node.style.border_bottom_left_radius.resolve(font_px, pr.w, 16.0) };
        let radius = r_tl.max(r_tr).max(r_br).max(r_bl);

        // ── Hover / active / visited check ───────────────────────────────────
        // CSS :hover/:active apply to an element whenever the pointer is over
        // it OR any of its descendants, so we check the whole subtree.
        // If a transition is running, node.style already has the interpolated values
        // applied via animation_overrides — skip hover_style so the transition shows.
        let node_id = node as *const HtmlBox as usize;
        let has_transition = self.transitioning_ids.contains(&node_id);
        let is_hovered = !has_transition
            && !hovered_ptr.is_null()
            && node.style.hover_style.is_some()
            && Self::subtree_contains(node, hovered_ptr);
        let is_active = !active_ptr.is_null()
            && node.style.active_style.is_some()
            && Self::subtree_contains(node, active_ptr);
        let is_visited = node.style.visited_style.is_some()
            && !node.style.href.is_empty()
            && visited_hrefs.contains(&node.style.href);
        // Effective style: active beats visited beats hover (most specific first).
        let eff_style: &ComputedStyle = if is_active {
            node.style.active_style.as_deref().unwrap_or(&node.style)
        } else if is_visited {
            node.style.visited_style.as_deref().unwrap_or(&node.style)
        } else if is_hovered {
            node.style.hover_style.as_deref().unwrap_or(&node.style)
        } else {
            &node.style
        };

        // ── Sticky positioning ────────────────────────────────────────────────
        // For position:sticky, clamp the element's scroll offset so it stays
        // within the viewport while still allowing normal flow scrolling.
        let (px, py) = if node.style.position == Position::Sticky {
            // top/left sticky thresholds relative to the current clip viewport.
            // clip.x/clip.y is the scroll container's top-left in screen space,
            // so the stick point is clip.origin + top/left value.
            let top_val  = node.style.top.resolve(font_px, clip.h, 16.0);
            let left_val = node.style.left.resolve(font_px, clip.w, 16.0);
            // Natural position (already scroll-adjusted above)
            let nat_x = pr.x - sx;
            let nat_y = pr.y - sy;
            // Clamp: don't scroll past the sticky threshold within the scroll container
            let cx = if !node.style.left.is_auto() { nat_x.max(clip.x + left_val) } else { nat_x };
            let cy = if !node.style.top.is_auto()  { nat_y.max(clip.y + top_val)  } else { nat_y };
            (cx, cy)
        } else {
            (px, py)
        };
        // Effective scroll offsets that account for sticky clamping.
        // For non-sticky elements: eff_sx == sx, eff_sy == sy.
        // For sticky elements: eff_sx/eff_sy are reduced so that children and inline
        // content are drawn at positions relative to the clamped (stuck) parent origin.
        let eff_sx = pr.x - px;
        let eff_sy = pr.y - py;

        // ── CSS transform ─────────────────────────────────────────────────────
        // Compute the element-level transform (CSS transform + DPI scale).
        let has_css_transform = !node.style.css_transform.ops.is_empty();
        let (elem_ts, css_t_for_text): (Transform, Option<Transform>) = if has_css_transform {
            let ox = px + node.style.transform_origin_x * pw;
            let oy = py + node.style.transform_origin_y * ph;
            let css_t = build_css_transform(&node.style.css_transform, ox, oy);
            // Combine: first apply CSS transform in logical coords, then scale to physical
            (Transform::from_scale(self.scale, self.scale).pre_concat(css_t), Some(css_t))
        } else {
            (Transform::from_scale(self.scale, self.scale), None)
        };

        // ── Clip-path ─────────────────────────────────────────────────────────
        // Build a pixmap-sized clip mask from clip-path shape (if any).
        let clip_path_mask = make_clip_path_mask(pixmap, node, px, py, pw, ph, font_px, self.scale);
        // Effective mask: prefer clip-path mask, fall back to parent mask.
        let eff_mask: Option<&Mask> = if let Some(ref m) = clip_path_mask {
            Some(m)
        } else {
            parent_mask
        };

        // ── Outer box-shadow (before background) ─────────────────────────────
        if let Some(ref bs) = eff_style.box_shadow {
            if !bs.inset {
                let shadow_x = px + bs.offset_x - bs.spread;
                let shadow_y = py + bs.offset_y - bs.spread;
                let shadow_w = pw + 2.0 * bs.spread;
                let shadow_h = ph + 2.0 * bs.spread;
                let layers   = ((bs.blur / 2.0) as i32).max(1);
                let base_a   = bs.color.a;
                for i in (0..=layers).rev() {
                    let la = ((base_a as i32) / (layers + 1)) as u8;
                    let sc = Color::rgba(bs.color.r, bs.color.g, bs.color.b, la);
                    let expand = i as f32;
                    let sx2 = shadow_x - expand;
                    let sy2 = shadow_y - expand;
                    let sw2 = shadow_w + 2.0 * expand;
                    let sh2 = shadow_h + 2.0 * expand;
                    let mut paint = Paint::default();
                    paint.set_color(sc.to_tiny_skia());
                    paint.anti_alias = true;
                    if radius > 0.0 {
                        if let Some(path) = rounded_rect_path_corners(sx2, sy2, sw2, sh2, r_tl, r_tr, r_br, r_bl) {
                            pixmap.fill_path(&path, &paint, FillRule::Winding, elem_ts, eff_mask);
                        }
                    } else if let Some(r) = SkRect::from_xywh(sx2, sy2, sw2, sh2) {
                        pixmap.fill_rect(r, &paint, elem_ts, eff_mask);
                    }
                }
            }
        }

        // ── Background ───────────────────────────────────────────────────────
        {
            let raw_bg  = eff_style.background_color;
            let opacity = eff_style.opacity;
            if raw_bg.a > 0 {
                let alpha = ((raw_bg.a as f32) * opacity) as u8;
                let bg = Color::rgba(raw_bg.r, raw_bg.g, raw_bg.b, alpha);
                let mut paint = Paint::default();
                paint.set_color(bg.to_tiny_skia());
                paint.anti_alias = true;
                paint.blend_mode = css_blend_mode(node.style.mix_blend_mode);
                if radius > 0.0 {
                    if let Some(path) = rounded_rect_path_corners(px, py, pw, ph, r_tl, r_tr, r_br, r_bl) {
                        pixmap.fill_path(&path, &paint, FillRule::Winding, elem_ts, eff_mask);
                    }
                } else if let Some(r) = SkRect::from_xywh(px, py, pw, ph) {
                    pixmap.fill_rect(r, &paint, elem_ts, eff_mask);
                }
            }
        }

        // ── Gradient background ──────────────────────────────────────────────
        if node.style.gradient_type != GradientType::None
            && node.style.gradient_stops.len() >= 2
        {
            self.draw_gradient(node, pixmap, px, py, pw, ph, radius, r_tl, r_tr, r_br, r_bl, elem_ts, eff_mask);
        }

        // ── Background image ─────────────────────────────────────────────────
        if let Some(ref bg_data) = node.bg_image_data {
            if node.bg_image_width > 0 && node.bg_image_height > 0 {
                self.draw_background_image(
                    bg_data, node.bg_image_width, node.bg_image_height,
                    &node.style, px, py, pw, ph, font_px,
                    radius, r_tl, r_tr, r_br, r_bl,
                    pixmap, elem_ts, eff_mask,
                );
            }
        }

        // ── Inset box-shadow (after background, before borders) ───────────────
        if let Some(ref bs) = node.style.box_shadow {
            if bs.inset {
                // Draw as a darker border-like effect inside the padding box.
                let layers = ((bs.blur / 2.0) as i32).max(1);
                let base_a = bs.color.a;
                for i in 0..=layers {
                    let la = ((base_a as i32) / (layers + 1)) as u8;
                    let sc = Color::rgba(bs.color.r, bs.color.g, bs.color.b, la);
                    let shrink = i as f32;
                    let ix = px + bs.offset_x + bs.spread + shrink;
                    let iy = py + bs.offset_y + bs.spread + shrink;
                    let iw = (pw - 2.0 * (bs.spread + shrink)).max(0.0);
                    let ih = (ph - 2.0 * (bs.spread + shrink)).max(0.0);
                    if iw < 1.0 || ih < 1.0 { break; }
                    let mut paint = Paint::default();
                    paint.set_color(sc.to_tiny_skia());
                    paint.anti_alias = true;
                    let mut stroke = Stroke::default();
                    stroke.width = 1.0;
                    if let Some(path) = rect_path(ix, iy, iw, ih) {
                        pixmap.stroke_path(&path, &paint, &stroke, elem_ts, eff_mask);
                    }
                }
            }
        }

        // ── Borders ──────────────────────────────────────────────────────────
        self.draw_borders_masked(node, eff_style, pixmap, eff_sx, eff_sy, elem_ts, eff_mask);

        // ── Outline ──────────────────────────────────────────────────────────
        if eff_style.outline_width > 0.0 && eff_style.outline_style != BorderStyle::None {
            let br2 = node.border_rect;
            let ofs = eff_style.outline_offset;
            let ow  = eff_style.outline_width;
            let rx  = br2.x - eff_sx - ofs - ow;
            let ry  = br2.y - eff_sy - ofs - ow;
            let rw  = br2.w + 2.0 * (ofs + ow);
            let rh  = br2.h + 2.0 * (ofs + ow);
            let mut paint = Paint::default();
            paint.set_color(eff_style.outline_color.to_tiny_skia());
            paint.anti_alias = true;
            let mut stroke = Stroke::default();
            stroke.width = ow;
            match eff_style.outline_style {
                BorderStyle::Dashed => {
                    draw_dashed_line(pixmap, &paint, ow, rx, ry, rx + rw, ry, self.scale);
                    draw_dashed_line(pixmap, &paint, ow, rx + rw, ry, rx + rw, ry + rh, self.scale);
                    draw_dashed_line(pixmap, &paint, ow, rx, ry + rh, rx + rw, ry + rh, self.scale);
                    draw_dashed_line(pixmap, &paint, ow, rx, ry, rx, ry + rh, self.scale);
                }
                BorderStyle::Dotted => {
                    draw_dotted_line(pixmap, &paint, ow, rx, ry, rx + rw, ry, self.scale);
                    draw_dotted_line(pixmap, &paint, ow, rx + rw, ry, rx + rw, ry + rh, self.scale);
                    draw_dotted_line(pixmap, &paint, ow, rx, ry + rh, rx + rw, ry + rh, self.scale);
                    draw_dotted_line(pixmap, &paint, ow, rx, ry, rx, ry + rh, self.scale);
                }
                _ => {
                    if let Some(path) = rect_path(rx, ry, rw, rh) {
                        pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), eff_mask);
                    }
                }
            }
        }

        // ── Build overflow clip for children ──────────────────────────────────
        // For overflow:hidden/scroll/auto, children are culled via a tighter clip rect.
        // We also build a Mask so pixel-level clips work for children that partially overlap.
        let overflow_clips = matches!(
            node.style.overflow_x, Overflow::Hidden | Overflow::Scroll | Overflow::Auto
        ) || matches!(
            node.style.overflow_y, Overflow::Hidden | Overflow::Scroll | Overflow::Auto
        );
        let overflow_mask = if overflow_clips {
            make_overflow_clip_mask(pixmap, px, py, pw, ph, radius, self.scale)
        } else {
            None
        };
        // Effective child mask: prefer overflow mask if present, else propagate eff_mask.
        let child_mask: Option<&Mask> = if let Some(ref om) = overflow_mask {
            Some(om)
        } else {
            eff_mask
        };

        // Tighter clip rect for children when overflow is clipping.
        let child_clip = if overflow_clips {
            let cx1 = px.max(clip.x);
            let cy1 = py.max(clip.y);
            let cx2 = (px + pw).min(clip.right());
            let cy2 = (py + ph).min(clip.bottom());
            Rect::new(cx1, cy1, (cx2 - cx1).max(0.0), (cy2 - cy1).max(0.0))
        } else {
            *clip
        };

        // ── Per-element scroll: children are shifted by the element's scroll ──
        // Use eff_sx/eff_sy so sticky elements keep their children aligned with
        // the clamped (stuck) background position.
        let child_sx = eff_sx + node.scroll_left;
        let child_sy = eff_sy + node.scroll_top;

        // ── ::before pseudo-element ───────────────────────────────────────────
        if !node.style.before_content.is_empty() && !node.line_cache.is_empty() {
            let first = &node.line_cache[0];
            let tx = first.x - eff_sx;
            let ty = first.y - eff_sy;
            let ps = node.style.before_style.as_deref().unwrap_or(&node.style);
            let ps_font_px = { let f = ps.font_size.resolve(font_px, 0.0, 16.0); if f > 0.0 { f } else { font_px } };
            let line_h = ps.line_height.resolve(ps_font_px, 0.0, 16.0).max(ps_font_px * 1.2);
            let fc = ps.color;
            let ct_col = CTextColor::rgba(fc.r, fc.g, fc.b, ((fc.a as f32) * ps.opacity) as u8);
            self.draw_text_run(
                &node.style.before_content.clone(), tx, ty, ps_font_px, line_h,
                ps.font_weight, ps.font_style, &ps.font_family, ct_col, pixmap, eff_mask,
            );
        }

        // ── Inline content (text lines + selection) ───────────────────────────
        // Use child_sx/child_sy so the element's own scroll_top/scroll_left shifts
        // the text, and child_mask so overflow clips apply to inline content too.
        //
        // When a CSS transform is active (e.g. scale() in heartbeat animation),
        // render text to a temporary pixmap first, then composite with the transform.
        // This ensures glyphs scale/rotate correctly — just drawing at a shifted x/y
        // position does not change glyph size.
        if !node.line_cache.is_empty() {
            let flat = collect_flat_text(node);
            if let Some(css_t) = css_t_for_text {
                // Render inline content into a transparent temp pixmap (same size).
                let sc = self.scale;
                if let Some(mut temp) = Pixmap::new(pixmap.width(), pixmap.height()) {
                    self.draw_inline_content(
                        node, eff_style, &flat, &mut temp, child_sx, child_sy,
                        sel_start, sel_end, sel_box_ptr,
                        None, // mask applied at composite time
                        is_hovered, is_active,
                    );
                    // T maps temp pixel coords → main pixel coords:
                    //   T = scale(sc) ∘ css_t ∘ scale(1/sc)
                    // In tiny_skia pre_concat (pre_concat(b) applies b first):
                    //   scale(sc).pre_concat(css_t).pre_concat(scale(1/sc))
                    let temp_to_main = Transform::from_scale(sc, sc)
                        .pre_concat(css_t)
                        .pre_concat(Transform::from_scale(1.0 / sc, 1.0 / sc));
                    pixmap.draw_pixmap(
                        0, 0, temp.as_ref(),
                        &tiny_skia::PixmapPaint::default(),
                        temp_to_main,
                        eff_mask,
                    );
                }
            } else {
                self.draw_inline_content(
                    node, eff_style, &flat, pixmap, child_sx, child_sy,
                    sel_start, sel_end, sel_box_ptr,
                    child_mask,
                    is_hovered, is_active,
                );
            }
        }

        // ── ::after pseudo-element ────────────────────────────────────────────
        if !node.style.after_content.is_empty() && !node.line_cache.is_empty() {
            let last = &node.line_cache[node.line_cache.len() - 1];
            let tx = last.x - eff_sx + last.width;
            let ty = last.y - eff_sy;
            let ps = node.style.after_style.as_deref().unwrap_or(&node.style);
            let ps_font_px = { let f = ps.font_size.resolve(font_px, 0.0, 16.0); if f > 0.0 { f } else { font_px } };
            let line_h = ps.line_height.resolve(ps_font_px, 0.0, 16.0).max(ps_font_px * 1.2);
            let fc = ps.color;
            let ct_col = CTextColor::rgba(fc.r, fc.g, fc.b, ((fc.a as f32) * ps.opacity) as u8);
            self.draw_text_run(
                &node.style.after_content.clone(), tx, ty, ps_font_px, line_h,
                ps.font_weight, ps.font_style, &ps.font_family, ct_col, pixmap, eff_mask,
            );
        }

        // ── List marker ──────────────────────────────────────────────────────
        if node.style.display == Display::ListItem && !node.line_cache.is_empty() {
            self.draw_list_marker(node, pixmap, eff_sx, eff_sy, eff_mask);
        }

        // ── HR ───────────────────────────────────────────────────────────────
        if node.tag == "hr" {
            self.draw_hr(node, pixmap, eff_sx, eff_sy, eff_mask);
        }

        // ── Custom Component Painting ────────────────────────────────────────
        if let Some(callbacks) = self.component_registry.map.get(&node.tag) {
            let cr = node.content_rect;
            (callbacks.paint)(node, pixmap, cr.x - eff_sx, cr.y - eff_sy, cr.w, cr.h, self.scale);
        }

        // ── Image placeholder for <img> ─────────────────────────────────────
        if node.tag == "img" {
            self.draw_image_placeholder(node, pixmap, eff_sx, eff_sy, eff_mask);
        }

        // ── Children: non-positioned first, then positioned by z-index ───────
        let has_positioned = node.children.iter().any(|c|
            c.style.is_positioned() && !matches!(c.style.display, Display::None));

        if !has_positioned {
            for child in &node.children {
                if !matches!(child.style.display, Display::None) {
                    self.render_box(
                        child, pixmap, child_sx, child_sy,
                        &child_clip, child_mask,
                        sel_start, sel_end, sel_box_ptr, hovered_ptr, active_ptr, visited_hrefs,
                    );
                }
            }
        } else {
            // Non-positioned first
            for child in &node.children {
                if !matches!(child.style.display, Display::None) && !child.style.is_positioned() {
                    self.render_box(
                        child, pixmap, child_sx, child_sy,
                        &child_clip, child_mask,
                        sel_start, sel_end, sel_box_ptr, hovered_ptr, active_ptr, visited_hrefs,
                    );
                }
            }
            // Positioned sorted by z-index
            let mut positioned: Vec<&HtmlBox> = node.children.iter()
                .filter(|c| c.style.is_positioned() && !matches!(c.style.display, Display::None))
                .collect();
            positioned.sort_by_key(|c| c.style.z_index);
            for child in positioned {
                match child.style.position {
                    Position::Fixed => {
                        // Fixed: always renders relative to viewport, never clipped by overflow.
                        self.render_box(
                            child, pixmap, 0.0, 0.0,
                            clip, eff_mask,
                            sel_start, sel_end, sel_box_ptr, hovered_ptr, active_ptr, visited_hrefs,
                        );
                    }
                    Position::Absolute => {
                        // Absolute: escapes overflow clip of non-positioned ancestors,
                        // but IS clipped by the nearest positioned overflow ancestor
                        // (which is this element if it has overflow != visible).
                        // Current node is its containing block only if positioned.
                        let (c, m) = if overflow_clips {
                            (&child_clip, child_mask)
                        } else {
                            (clip, eff_mask)
                        };
                        self.render_box(
                            child, pixmap, child_sx, child_sy,
                            c, m,
                            sel_start, sel_end, sel_box_ptr, hovered_ptr, active_ptr, visited_hrefs,
                        );
                    }
                    _ => {
                        // Relative / sticky: stays in flow visually, MUST be clipped by
                        // parent overflow just like a normal in-flow child.
                        self.render_box(
                            child, pixmap, child_sx, child_sy,
                            &child_clip, child_mask,
                            sel_start, sel_end, sel_box_ptr, hovered_ptr, active_ptr, visited_hrefs,
                        );
                    }
                }
            }
        }

        // ── Scrollbars ────────────────────────────────────────────────────────
        self.draw_scrollbars(node, pixmap, eff_sx, eff_sy);

        // ── CSS Filters ───────────────────────────────────────────────────────
        // Apply pixel-level filter ops (blur, brightness, etc.) to the element region.
        if !node.style.css_filter.ops.is_empty() {
            apply_css_filters(pixmap, &node.style.css_filter, px, py, pw, ph, radius, self.scale);
        }
    }

    // ─── State helpers ────────────────────────────────────────────────────────

    /// True when `target` is `node` itself or any descendant of `node`.
    /// Used so that CSS `:hover`/`:active` activate on a parent element whenever
    /// the cursor is over any child — matching CSS cascade semantics.
    /// Only called for nodes that actually have a state style, so the cost is
    /// bounded to the small subset of nodes with hover/active rules.
    fn subtree_contains(node: &HtmlBox, target: *const HtmlBox) -> bool {
        if std::ptr::eq(node as *const HtmlBox, target) { return true; }
        for child in &node.children {
            if Self::subtree_contains(child, target) { return true; }
        }
        false
    }

    // ─── Borders (mask-aware) ────────────────────────────────────────────────

    fn draw_borders_masked(
        &self,
        node:   &HtmlBox,
        style:  &ComputedStyle,
        pixmap: &mut Pixmap,
        sx:     f32,
        sy:     f32,
        elem_ts: Transform,
        mask:   Option<&Mask>,
    ) {
        let br      = node.border_rect;
        let font_px = style.font_size_px(16.0, 16.0);
        let r_shorthand = style.border_radius.resolve(font_px, br.w, 16.0);
        let r_tl = if r_shorthand > 0.0 { r_shorthand }
            else { style.border_top_left_radius.resolve(font_px, br.w, 16.0) };
        let r_tr = if r_shorthand > 0.0 { r_shorthand }
            else { style.border_top_right_radius.resolve(font_px, br.w, 16.0) };
        let r_br = if r_shorthand > 0.0 { r_shorthand }
            else { style.border_bottom_right_radius.resolve(font_px, br.w, 16.0) };
        let r_bl = if r_shorthand > 0.0 { r_shorthand }
            else { style.border_bottom_left_radius.resolve(font_px, br.w, 16.0) };
        let radius = r_tl.max(r_tr).max(r_br).max(r_bl);
        let rx      = br.x - sx;
        let ry      = br.y - sy;

        let all_same = style.border_top_style    == style.border_right_style
            && style.border_right_style  == style.border_bottom_style
            && style.border_bottom_style == style.border_left_style
            && style.border_top_color    == style.border_right_color
            && style.border_right_color  == style.border_bottom_color
            && style.border_bottom_color == style.border_left_color;

        let opacity = style.opacity;

        if all_same && style.border_top_style != BorderStyle::None {
            let tw = style.border_top_width.resolve(font_px, br.w, 16.0).max(1.0);
            let c  = style.border_top_color;
            let ca = ((c.a as f32) * opacity) as u8;
            let mut paint = Paint::default();
            paint.set_color(Color::rgba(c.r, c.g, c.b, ca).to_tiny_skia());
            paint.anti_alias = true;
            let mut stroke = Stroke::default();
            stroke.width = tw;

            if radius > 0.0 {
                if let Some(path) = rounded_rect_path_corners(
                    rx + tw/2.0, ry + tw/2.0, br.w - tw, br.h - tw, r_tl, r_tr, r_br, r_bl,
                ) {
                    pixmap.stroke_path(&path, &paint, &stroke, elem_ts, mask);
                }
            } else if let Some(path) = rect_path(rx, ry, br.w, br.h) {
                pixmap.stroke_path(&path, &paint, &stroke, elem_ts, mask);
            }
        } else if radius > 0.0 {
            // Per-side with border-radius: build separate arc path for each side using
            // PathBuilder so that CSS transforms (including rotation) work correctly.
            // The clip-mask approach breaks under rotation because masks live in screen space.
            const K: f32 = 0.5522847498; // cubic bezier factor for quarter-circle approximation

            let sides = [
                (Side::Top,    &style.border_top_width,    style.border_top_style,    style.border_top_color),
                (Side::Bottom, &style.border_bottom_width, style.border_bottom_style, style.border_bottom_color),
                (Side::Left,   &style.border_left_width,   style.border_left_style,   style.border_left_color),
                (Side::Right,  &style.border_right_width,  style.border_right_style,  style.border_right_color),
            ];

            for (side, width_l, bstyle, color) in &sides {
                if *bstyle == BorderStyle::None || *bstyle == BorderStyle::Hidden { continue; }
                let ca = ((color.a as f32) * opacity) as u8;
                if ca == 0 { continue; }
                let w = width_l.resolve(font_px, br.w, 16.0);
                if w < 0.5 { continue; }

                let ax = rx + w / 2.0;
                let ay = ry + w / 2.0;
                let aw = br.w - w;
                let ah = br.h - w;
                let r = radius.min(aw / 2.0).min(ah / 2.0);

                let mut pb = PathBuilder::new();
                match side {
                    Side::Top => {
                        // top-left corner arc + top edge + top-right corner arc
                        pb.move_to(ax, ay + r);
                        pb.cubic_to(ax, ay + r - K*r, ax + r - K*r, ay, ax + r, ay);
                        pb.line_to(ax + aw - r, ay);
                        pb.cubic_to(ax + aw - r + K*r, ay, ax + aw, ay + r - K*r, ax + aw, ay + r);
                    }
                    Side::Bottom => {
                        // bottom-right corner arc + bottom edge + bottom-left corner arc
                        pb.move_to(ax + aw, ay + ah - r);
                        pb.cubic_to(ax + aw, ay + ah - r + K*r, ax + aw - r + K*r, ay + ah, ax + aw - r, ay + ah);
                        pb.line_to(ax + r, ay + ah);
                        pb.cubic_to(ax + r - K*r, ay + ah, ax, ay + ah - r + K*r, ax, ay + ah - r);
                    }
                    Side::Left => {
                        // straight left edge between corner arcs (corners owned by top/bottom)
                        pb.move_to(ax, ay + r);
                        pb.line_to(ax, ay + ah - r);
                    }
                    Side::Right => {
                        // straight right edge between corner arcs
                        pb.move_to(ax + aw, ay + r);
                        pb.line_to(ax + aw, ay + ah - r);
                    }
                }

                let path = match pb.finish() {
                    Some(p) => p,
                    None => continue,
                };

                let mut paint = Paint::default();
                paint.set_color(Color::rgba(color.r, color.g, color.b, ca).to_tiny_skia());
                paint.anti_alias = true;
                let mut stroke = Stroke::default();
                stroke.width = w;
                pixmap.stroke_path(&path, &paint, &stroke, elem_ts, mask);
            }
        } else {
            self.draw_border_side_masked(pixmap, sx, sy, node, style, Side::Top,    opacity, elem_ts, mask);
            self.draw_border_side_masked(pixmap, sx, sy, node, style, Side::Right,  opacity, elem_ts, mask);
            self.draw_border_side_masked(pixmap, sx, sy, node, style, Side::Bottom, opacity, elem_ts, mask);
            self.draw_border_side_masked(pixmap, sx, sy, node, style, Side::Left,   opacity, elem_ts, mask);
        }
    }

    fn draw_border_side_masked(
        &self,
        pixmap:  &mut Pixmap,
        sx:      f32,
        sy:      f32,
        node:    &HtmlBox,
        style:   &ComputedStyle,
        side:    Side,
        opacity: f32,
        elem_ts: Transform,
        mask:    Option<&Mask>,
    ) {
        let (bstyle, color, width_l) = match side {
            Side::Top    => (style.border_top_style,    style.border_top_color,    &style.border_top_width),
            Side::Right  => (style.border_right_style,  style.border_right_color,  &style.border_right_width),
            Side::Bottom => (style.border_bottom_style, style.border_bottom_color, &style.border_bottom_width),
            Side::Left   => (style.border_left_style,   style.border_left_color,   &style.border_left_width),
        };
        if bstyle == BorderStyle::None || bstyle == BorderStyle::Hidden { return; }
        let font_px = style.font_size_px(16.0, 16.0);
        let w = width_l.resolve(font_px, node.border_rect.w, 16.0);
        if w < 0.5 { return; }

        let br = node.border_rect;
        let rx = br.x - sx;
        let ry = br.y - sy;
        let ca = ((color.a as f32) * opacity) as u8;
        let color2 = Color::rgba(color.r, color.g, color.b, ca);

        let (x1, y1, x2, y2) = match side {
            Side::Top    => (rx,                ry + w/2.0,           rx + br.w,          ry + w/2.0),
            Side::Bottom => (rx,                ry + br.h - w/2.0,    rx + br.w,          ry + br.h - w/2.0),
            Side::Left   => (rx + w/2.0,        ry,                   rx + w/2.0,         ry + br.h),
            Side::Right  => (rx + br.w - w/2.0, ry,                   rx + br.w - w/2.0,  ry + br.h),
        };

        let mut paint = Paint::default();
        paint.set_color(color2.to_tiny_skia());
        paint.anti_alias = true;
        let mut stroke = Stroke::default();
        stroke.width = w;
        stroke.line_cap = LineCap::Square;

        match bstyle {
            BorderStyle::Dashed => draw_dashed_line(pixmap, &paint, w, x1, y1, x2, y2, self.scale),
            BorderStyle::Dotted => draw_dotted_line(pixmap, &paint, w, x1, y1, x2, y2, self.scale),
            BorderStyle::Double => {
                let third = w / 3.0;
                let mut s2 = Stroke::default();
                s2.width = third;
                if let Some(path) = line_path(x1, y1, x2, y2) {
                    pixmap.stroke_path(&path, &paint, &s2, elem_ts, mask);
                }
                let (ix1, iy1, ix2, iy2) = match side {
                    Side::Top    => (x1, y1 + 2.0*third, x2, y2 + 2.0*third),
                    Side::Bottom => (x1, y1 - 2.0*third, x2, y2 - 2.0*third),
                    Side::Left   => (x1 + 2.0*third, y1, x2 + 2.0*third, y2),
                    Side::Right  => (x1 - 2.0*third, y1, x2 - 2.0*third, y2),
                };
                if let Some(path) = line_path(ix1, iy1, ix2, iy2) {
                    pixmap.stroke_path(&path, &paint, &s2, elem_ts, mask);
                }
            }
            _ => {
                if let Some(path) = line_path(x1, y1, x2, y2) {
                    pixmap.stroke_path(&path, &paint, &stroke, elem_ts, mask);
                }
            }
        }
    }

    // ─── Gradient ────────────────────────────────────────────────────────────

    fn draw_gradient(
        &self,
        node: &HtmlBox,
        pixmap: &mut Pixmap,
        px: f32, py: f32, pw: f32, ph: f32,
        radius: f32, r_tl: f32, r_tr: f32, r_br: f32, r_bl: f32,
        elem_ts: Transform,
        mask: Option<&Mask>,
    ) {
        use tiny_skia::{
            LinearGradient, RadialGradient,
            GradientStop as SkStop, SpreadMode, Point as SkPoint,
        };

        if pw <= 0.0 || ph <= 0.0 { return; }

        // Convert our gradient stops to tiny-skia's format, applying element opacity.
        let opacity = node.style.opacity;
        let sk_stops: Vec<SkStop> = node.style.gradient_stops.iter()
            .map(|s| {
                let a = ((s.color.a as f32) * opacity) as u8;
                SkStop::new(s.position, tiny_skia::Color::from_rgba8(s.color.r, s.color.g, s.color.b, a))
            })
            .collect();
        if sk_stops.len() < 2 { return; }

        let mut paint = Paint::default();
        paint.anti_alias = true;
        paint.blend_mode = css_blend_mode(node.style.mix_blend_mode);

        let shader = match node.style.gradient_type {
            GradientType::Linear => {
                let angle = node.style.gradient_angle;
                let rad = angle * std::f32::consts::PI / 180.0;
                let dx = rad.sin();
                let dy = -rad.cos();

                // The old per-pixel code computed t from normalised (nx, ny) in [0,1]:
                //   t = (nx*dx + ny*dy - t_min) / t_range
                // In physical coords (x = px + nx*pw, y = py + ny*ph) that is:
                //   t = (x-px)*dx/(pw*t_range) + (y-py)*dy/(ph*t_range) - t_min/t_range
                //
                // tiny-skia's LinearGradient computes:
                //   t = dot(P - start, end - start) / |end - start|^2
                //
                // Matching the two: the end-start vector must equal
                //   (dx * pw * ph^2 * t_range, dy * ph * pw^2 * t_range) / (dx^2*ph^2 + dy^2*pw^2)
                // Start = corner of physical box that achieves t_min.
                let corners = [0.0f32, dx, dy, dx + dy];
                let t_min = corners.iter().cloned().fold(f32::MAX, f32::min);
                let t_max = corners.iter().cloned().fold(f32::MIN, f32::max);
                let t_range = (t_max - t_min).max(0.001);

                let start_nx: f32 = if dx >= 0.0 { 0.0 } else { 1.0 };
                let start_ny: f32 = if dy >= 0.0 { 0.0 } else { 1.0 };
                let sx = px + start_nx * pw;
                let sy = py + start_ny * ph;

                let denom = dx * dx * ph * ph + dy * dy * pw * pw;
                let (ex, ey) = if denom > 1e-6 {
                    (sx + dx * pw * ph * ph * t_range / denom,
                     sy + dy * ph * pw * pw * t_range / denom)
                } else {
                    (sx + pw, sy)  // degenerate: horizontal fallback
                };

                LinearGradient::new(
                    SkPoint::from_xy(sx, sy), SkPoint::from_xy(ex, ey),
                    sk_stops, SpreadMode::Pad, Transform::identity(),
                )
            }
            GradientType::Radial => {
                let cx = px + pw / 2.0;
                let cy = py + ph / 2.0;
                // Radius = distance from centre to corner, matching the old implementation.
                let r = ((pw / 2.0).powi(2) + (ph / 2.0).powi(2)).sqrt().max(1.0);
                let center = SkPoint::from_xy(cx, cy);
                RadialGradient::new(center, 0.0, center, r, sk_stops, SpreadMode::Pad, Transform::identity())
            }
            GradientType::None => return,
        };

        paint.shader = match shader {
            Some(s) => s,
            None => return,
        };

        // Draw via fill_path / fill_rect so elem_ts (CSS transform + DPI scale)
        // is applied automatically — gradients now transform correctly.
        if radius > 0.0 {
            if let Some(path) = rounded_rect_path_corners(px, py, pw, ph, r_tl, r_tr, r_br, r_bl) {
                pixmap.fill_path(&path, &paint, FillRule::Winding, elem_ts, mask);
            }
        } else if let Some(r) = SkRect::from_xywh(px, py, pw, ph) {
            pixmap.fill_rect(r, &paint, elem_ts, mask);
        }
    }

    // ─── Inline content ───────────────────────────────────────────────────────

    fn draw_inline_content(
        &mut self,
        node:       &HtmlBox,
        eff_style:  &ComputedStyle,
        flat:       &str,
        pixmap:     &mut Pixmap,
        sx:         f32,
        sy:         f32,
        sel_start:  Option<usize>,
        sel_end:    Option<usize>,
        sel_box_ptr: *const HtmlBox,
        mask:       Option<&Mask>,
        is_hovered: bool,
        is_active:  bool,
    ) {
        if node.line_cache.is_empty() || flat.is_empty() { return; }

        let opacity             = eff_style.opacity;
        let fallback_font_px    = node.style.font_size_px(16.0, 16.0);
        let fallback_letter_spc = node.style.letter_spacing.resolve(fallback_font_px, 0.0, 16.0);
        let fallback_color      = eff_style.color;
        let _fallback_ct_color = CTextColor::rgba(
            fallback_color.r, fallback_color.g, fallback_color.b,
            ((fallback_color.a as f32) * opacity) as u8,
        );

        let is_sel_box = !sel_box_ptr.is_null()
            && std::ptr::eq(node as *const HtmlBox, sel_box_ptr);
        let (sel_min, sel_max) = if is_sel_box {
            match (sel_start, sel_end) {
                (Some(s), Some(e)) => (s.min(e), s.max(e)),
                _ => (0, 0),
            }
        } else {
            (0, 0)
        };

        let use_ellipsis = node.style.text_overflow == TextOverflow::Ellipsis
            && (node.style.overflow_x == Overflow::Hidden
                || node.style.overflow_x == Overflow::Scroll);
        // Right edge of the padding box in screen coordinates.
        // cursor_x is also in screen coords (line.x - sx), so we compare against this.
        let max_content_right = (node.padding_rect.x - sx) + node.padding_rect.w;

        for line in node.line_cache.clone() {
            let line_start = floor_char_boundary(flat, line.text_start.min(flat.len()));
            let line_end   = floor_char_boundary(flat, (line.text_start + line.text_length).min(flat.len()));
            if line_start >= line_end { continue; }
            if flat[line_start..line_end].trim().is_empty() { continue; }

            let lx = line.x - sx;
            let ly = line.y - sy;

            // ── Selection highlight ──────────────────────────────────────────
            if sel_min < sel_max && sel_min < line_end && sel_max > line_start {
                let h_start = sel_min.max(line_start);
                let h_end   = sel_max.min(line_end);
                if h_start < h_end {
                    let mut sel_paint = Paint::default();
                    if let Some(ss) = node.style.selection_style.as_deref() {
                        let bg = ss.background_color;
                        sel_paint.set_color_rgba8(bg.r, bg.g, bg.b, if bg.a > 0 { bg.a } else { 200 });
                    } else {
                        sel_paint.set_color_rgba8(200, 220, 255, 200);
                    }
                    if !line.visual_segments.is_empty() {
                        for vs in &line.visual_segments {
                            let seg_s = vs.logical_start;
                            let seg_e = vs.logical_start + vs.length;
                            let hl_s  = h_start.max(seg_s);
                            let hl_e  = h_end.min(seg_e);
                            if hl_s >= hl_e { continue; }
                            let frac_s = if vs.length > 0 { (hl_s - seg_s) as f32 / vs.length as f32 } else { 0.0 };
                            let frac_e = if vs.length > 0 { (hl_e - seg_s) as f32 / vs.length as f32 } else { 1.0 };
                            let xs = vs.x + frac_s * vs.width;
                            let xe = vs.x + frac_e * vs.width;
                            let (xl, xr) = if xs < xe { (xs, xe) } else { (xe, xs) };
                            if let Some(r) = SkRect::from_xywh(lx + xl, ly, xr - xl, line.height) {
                                pixmap.fill_rect(r, &sel_paint, Transform::from_scale(self.scale, self.scale), mask);
                            }
                        }
                    } else if !line.char_x.is_empty() {
                        // Use shaped per-character x positions for pixel-accurate highlights.
                        let i_s = (h_start - line_start).min(line.char_x.len() - 1);
                        let i_e = (h_end   - line_start).min(line.char_x.len() - 1);
                        let xs = lx + line.char_x[i_s];
                        let xe = lx + line.char_x[i_e];
                        if xe > xs {
                            if let Some(r) = SkRect::from_xywh(xs, ly, xe - xs, line.height) {
                                pixmap.fill_rect(r, &sel_paint, Transform::from_scale(self.scale, self.scale), mask);
                            }
                        }
                    } else {
                        // Fallback (no char_x): byte-ratio approximation.
                        let len     = line_end - line_start;
                        let ratio_s = if len > 0 { (h_start - line_start) as f32 / len as f32 } else { 0.0 };
                        let ratio_e = if len > 0 { (h_end   - line_start) as f32 / len as f32 } else { 1.0 };
                        let xs = lx + ratio_s * line.width;
                        let xe = lx + ratio_e * line.width;
                        if let Some(r) = SkRect::from_xywh(xs, ly, xe - xs, line.height) {
                            pixmap.fill_rect(r, &sel_paint, Transform::from_scale(self.scale, self.scale), mask);
                        }
                    }
                }
            }

            // ── Text rendering ───────────────────────────────────────────────
            // Build rendering order: visual segments if BiDi, otherwise logical.
            struct Chunk { s: usize, e: usize, run_idx: Option<usize>, #[allow(dead_code)] rtl: bool }
            let mut chunks: Vec<Chunk> = Vec::new();

            if !line.visual_segments.is_empty() && !node.inline_runs.is_empty() {
                for vs in &line.visual_segments {
                    let seg_s   = vs.logical_start;
                    let seg_e   = vs.logical_start + vs.length;
                    let is_rtl  = (vs.level & 1) != 0;
                    let mut seg_chunks: Vec<Chunk> = Vec::new();
                    for (ri, run) in node.inline_runs.iter().enumerate() {
                        let rs  = run.text_offset;
                        let re  = rs + run.length;
                        let cs  = seg_s.max(rs);
                        let ce  = seg_e.min(re);
                        if cs < ce {
                            seg_chunks.push(Chunk { s: cs, e: ce, run_idx: Some(ri), rtl: is_rtl });
                        }
                    }
                    if is_rtl {
                        // Before reversing, trim any leading ASCII spaces from the
                        // last chunk in logical order.  After reversal it becomes the
                        // visual-first (leftmost) chunk; a leading space there creates
                        // an unwanted gap at the left edge of RTL lines (e.g. the " ."
                        // produced by whitespace-collapsing "\n." at the end of an Arabic
                        // sentence would otherwise render as "[ ][.] مائلة و").
                        if let Some(last) = seg_chunks.last_mut() {
                            // Trim any ASCII whitespace (space, tab, newline, CR) from the
                            // start of the last chunk.  In the flat text, whitespace-collapsing
                            // of e.g. "\n." produces a raw '\n' byte followed by '.'.
                            // After RTL reversal this chunk becomes the visual-first (leftmost)
                            // one; a leading whitespace byte is rendered as a space by
                            // cosmic-text, creating an unwanted gap at the left edge.
                            while last.s < last.e
                                && last.s < flat.len()
                                && matches!(flat.as_bytes()[last.s], b' ' | b'\t' | b'\n' | b'\r')
                            {
                                last.s += 1;
                            }
                        }
                        seg_chunks.reverse();
                    }
                    chunks.extend(seg_chunks);
                }
            } else if node.inline_runs.is_empty() {
                chunks.push(Chunk { s: line_start, e: line_end, run_idx: None, rtl: false });
            } else {
                for (ri, run) in node.inline_runs.iter().enumerate() {
                    let cs = line_start.max(run.text_offset);
                    let ce = line_end.min(run.text_offset + run.length);
                    if cs < ce {
                        chunks.push(Chunk { s: cs, e: ce, run_idx: Some(ri), rtl: false });
                    }
                }
            }

            let mut cursor_x = lx;

            for chunk in &chunks {
                let s = floor_char_boundary(flat, chunk.s);
                let e = floor_char_boundary(flat, chunk.e);
                if e <= s { continue; }

                let (run_style, run_font_px, run_letter_spc, run_word_spc, run_extra) =
                    if let Some(ri) = chunk.run_idx {
                        let run = &node.inline_runs[ri];
                        let fp  = run.style.font_size_px(16.0, 16.0);
                        let ls  = run.style.letter_spacing.resolve(fp, 0.0, 16.0);
                        let ws  = run.style.word_spacing.resolve(fp, 0.0, 16.0);
                        (Some(&run.style), fp, ls, ws, line.extra_space_per_word)
                    } else {
                        (None, fallback_font_px, fallback_letter_spc, 0.0, line.extra_space_per_word)
                    };

                let style_ref: &ComputedStyle = run_style.unwrap_or(&node.style);
                // Arabic and other RTL scripts have no italic variant in most fonts.
                // Requesting italic for RTL text causes cosmic-text to select a Latin
                // italic font that cannot render Arabic glyphs (→ tofu / "weird lines").
                // Suppress italic for RTL chunks so the Arabic font is always used.
                let effective_font_style = if chunk.rtl {
                    FontStyle::Normal
                } else {
                    style_ref.font_style
                };
                let seg_text = &flat[s..e];
                // Normalize raw newlines (HTML source formatting) to spaces so
                // cosmic-text doesn't split the run into two lines, displacing
                // subsequent characters (visible as "horizontal line" artifacts
                // in RTL/Arabic text).
                let seg_text_clean: String;
                let seg_text_for_draw: &str = if seg_text.contains('\n') || seg_text.contains('\r') {
                    seg_text_clean = seg_text.chars()
                        .map(|c| if matches!(c, '\n' | '\r') { ' ' } else { c })
                        .collect();
                    &seg_text_clean
                } else {
                    seg_text
                };
                let draw_text = apply_text_transform(seg_text_for_draw, style_ref.text_transform);

                let run_line_h = style_ref.line_height.resolve(run_font_px, 0.0, 16.0)
                    .max(run_font_px * 1.2);

                // Approx width for pre-draw uses (background rect, ellipsis check).
                // The true advance is returned by draw_text_run after shaping.
                let approx_seg_w = approx_text_width_ls(&draw_text, run_font_px, run_letter_spc);

                // Run background color — use the actual shaped advance so that
                // Arabic / complex-script text (where approx_seg_w overestimates
                // because it uses 1× font_px per char) doesn't produce an oversized
                // background rectangle that covers adjacent chunks.
                if style_ref.background_color.a > 0 {
                    let bg_w = self.measure_text_run(
                        &draw_text, run_font_px, run_line_h,
                        style_ref.font_weight, effective_font_style,
                    );
                    let mut bp = Paint::default();
                    bp.set_color(style_ref.background_color.to_tiny_skia());
                    if let Some(r) = SkRect::from_xywh(cursor_x, ly, bg_w, line.height) {
                        pixmap.fill_rect(r, &bp, Transform::from_scale(self.scale, self.scale), mask);
                    }
                }

                // Text color: use effective state style's color when:
                // 1. The run IS the node's own style (ptr::eq), or
                // 2. The node is hovered/active and the run's color equals the node's
                //    base cascade color — meaning the run inherited its color from this
                //    node and should pick up the hover/transition color.
                //    Runs with an explicit own color (e.g. <a> links) differ from
                //    node.style.color and correctly keep their own value.
                let run_color = if std::ptr::eq(style_ref as *const _, &node.style as *const _)
                    || ((is_hovered || is_active) && style_ref.color == node.style.color)
                {
                    eff_style.color
                } else {
                    style_ref.color
                };
                let ct_color = CTextColor::rgba(
                    run_color.r, run_color.g, run_color.b,
                    ((run_color.a as f32) * opacity) as u8,
                );

                // text-overflow: ellipsis check
                let final_text = if use_ellipsis && cursor_x + approx_seg_w > max_content_right {
                    let avail = max_content_right - cursor_x;
                    if avail > 0.0 {
                        truncate_with_ellipsis(&draw_text, run_font_px, run_letter_spc, avail)
                    } else {
                        String::from("…")
                    }
                } else {
                    draw_text.clone()
                };

                // Text shadow (with blur approximation)
                if let Some(ref ts) = style_ref.text_shadow {
                    let blur = ts.blur.max(0.0);
                    if blur < 1.0 {
                        // No blur — single pass
                        let sh = CTextColor::rgba(ts.color.r, ts.color.g, ts.color.b, ts.color.a);
                        self.draw_text_run_ex(
                            &final_text,
                            cursor_x + ts.offset_x, ly + ts.offset_y,
                            run_font_px, run_line_h,
                            style_ref.font_weight, effective_font_style,
                            &style_ref.font_family,
                            style_ref.font_stretch,
                            &style_ref.font_variation_settings,
                            sh, pixmap, mask,
                        );
                    } else {
                        // Approximate blur with multiple offset passes
                        let steps = (blur / 1.5).ceil().min(5.0) as i32;
                        let alpha_div = (steps * 2 + 1) as u16;
                        let layer_a = ((ts.color.a as u16 * 2) / alpha_div).max(1) as u8;
                        let sh = CTextColor::rgba(ts.color.r, ts.color.g, ts.color.b, layer_a);
                        for dy in -steps..=steps {
                            let oy = ts.offset_y + (dy as f32) * (blur / steps as f32) * 0.5;
                            self.draw_text_run_ex(
                                &final_text,
                                cursor_x + ts.offset_x, ly + oy,
                                run_font_px, run_line_h,
                                style_ref.font_weight, effective_font_style,
                                &style_ref.font_family,
                                style_ref.font_stretch,
                                &style_ref.font_variation_settings,
                                sh, pixmap, mask,
                            );
                        }
                    }
                }

                // Main text — returns the actual cosmic-text advance (logical pixels).
                let actual_advance = self.draw_text_run_ex(
                    &final_text, cursor_x, ly,
                    run_font_px, run_line_h,
                    style_ref.font_weight, effective_font_style,
                    &style_ref.font_family,
                    style_ref.font_stretch,
                    &style_ref.font_variation_settings,
                    ct_color, pixmap, mask,
                );

                // Use actual rendered width for decorations and cursor advance.
                // Add CSS word-spacing and justify extra-space per space character.
                let n_spaces = draw_text.chars().filter(|&c| c == ' ').count() as f32;
                let seg_w = actual_advance + n_spaces * (run_word_spc + run_extra);

                // Per-segment text decorations
                self.draw_text_decorations_segment(
                    style_ref, cursor_x, ly, seg_w, line.height, line.ascent, opacity, pixmap, mask,
                );

                cursor_x += seg_w;
            }

            // Whole-line decorations (when no per-run decorations)
            if node.inline_runs.is_empty() {
                self.draw_text_decorations_line(node, &line, lx, ly, opacity, pixmap, mask);
            }
        }
    }

    // ─── Text decorations (per segment) ──────────────────────────────────────

    fn draw_text_decorations_segment(
        &self,
        style:   &ComputedStyle,
        x:       f32,
        y:       f32,
        width:   f32,
        height:  f32,
        ascent:  f32,
        opacity: f32,
        pixmap:  &mut Pixmap,
        mask:    Option<&Mask>,
    ) {
        let dec = style.text_decoration;
        if !dec.underline && !dec.strikethrough && !dec.overline { return; }
        let font_px    = style.font_size_px(16.0, 16.0);
        // Resolve line thickness: text-decoration-thickness or default
        let line_thick = {
            let t = style.text_decoration_thickness.resolve(font_px, font_px, 16.0);
            if t > 0.0 { t } else { (font_px / 12.0).max(1.0) }
        };
        // Decoration color: text-decoration-color or fallback to text color
        let color      = style.text_decoration_color.unwrap_or(style.color);
        let alpha      = ((color.a as f32) * opacity) as u8;
        let dec_color  = Color::rgba(color.r, color.g, color.b, alpha);
        let dec_style  = style.text_decoration_style;
        let ts         = Transform::from_scale(self.scale, self.scale);

        // Helper closure to draw a decoration line using the correct style
        let draw_deco_line = |pixmap: &mut Pixmap, lx: f32, ly: f32, lw: f32, lh: f32| {
            let mut paint = Paint::default();
            paint.set_color(dec_color.to_tiny_skia());
            paint.anti_alias = true;
            match dec_style {
                TextDecorationStyle::Solid | TextDecorationStyle::Double => {
                    if let Some(r) = SkRect::from_xywh(lx, ly, lw, lh) {
                        pixmap.fill_rect(r, &paint, ts, mask);
                    }
                    if dec_style == TextDecorationStyle::Double {
                        // Draw a second line below/above with a gap
                        let gap = lh + 1.0;
                        if let Some(r) = SkRect::from_xywh(lx, ly + gap, lw, lh) {
                            pixmap.fill_rect(r, &paint, ts, mask);
                        }
                    }
                }
                TextDecorationStyle::Dotted => {
                    draw_dotted_line(pixmap, &paint, lh, lx, ly + lh / 2.0, lx + lw, ly + lh / 2.0, self.scale);
                }
                TextDecorationStyle::Dashed => {
                    draw_dashed_line(pixmap, &paint, lh, lx, ly + lh / 2.0, lx + lw, ly + lh / 2.0, self.scale);
                }
                TextDecorationStyle::Wavy => {
                    // Draw a wavy line using short strokes approximating a sine wave
                    let amplitude = lh.max(1.0);
                    let period = (font_px * 0.4).max(4.0);
                    let mut stroke = Stroke::default();
                    stroke.width = (lh * 0.6).max(0.5);
                    let cy = ly + lh / 2.0;
                    let mut px2 = lx;
                    while px2 < lx + lw {
                        let t1 = (px2 - lx) / period * std::f32::consts::TAU;
                        let t2 = ((px2 + period * 0.25) - lx) / period * std::f32::consts::TAU;
                        let y1 = cy + t1.sin() * amplitude;
                        let y2 = cy + t2.sin() * amplitude;
                        if let Some(path) = line_path(px2, y1, (px2 + period * 0.25).min(lx + lw), y2) {
                            pixmap.stroke_path(&path, &paint, &stroke, ts, mask);
                        }
                        px2 += period * 0.25;
                    }
                }
            }
        };

        if dec.underline {
            // Resolve underline offset: text-underline-offset or default
            let offset = style.text_underline_offset.resolve(font_px, font_px, 16.0);
            let offset = if offset > 0.0 { offset } else { line_thick * 2.0 };
            let uy = y + ascent + offset;
            draw_deco_line(pixmap, x, uy, width, line_thick);
        }
        if dec.strikethrough {
            // Strike-through at ~30% of em above the baseline (center of x-height).
            let sy2 = y + ascent - font_px * 0.30;
            draw_deco_line(pixmap, x, sy2, width, line_thick);
        }
        if dec.overline {
            draw_deco_line(pixmap, x, y, width, line_thick);
        }
        let _ = height;
    }

    fn draw_text_decorations_line(
        &self,
        node:   &HtmlBox,
        line:   &LayoutLine,
        ox:     f32,
        oy:     f32,
        opacity: f32,
        pixmap: &mut Pixmap,
        mask:   Option<&Mask>,
    ) {
        let dec = node.style.text_decoration;
        if !dec.underline && !dec.strikethrough && !dec.overline { return; }

        let font_px  = node.style.font_size_px(16.0, 16.0);
        let lw       = line.width;
        // Resolve line thickness
        let line_thick = {
            let t = node.style.text_decoration_thickness.resolve(font_px, font_px, 16.0);
            if t > 0.0 { t } else { (font_px * 0.08).max(1.0) }
        };
        // Decoration color
        let c  = node.style.text_decoration_color.unwrap_or(node.style.color);
        let ca = ((c.a as f32) * opacity) as u8;
        let dec_color = Color::rgba(c.r, c.g, c.b, ca);
        let dec_style = node.style.text_decoration_style;
        let ts = Transform::from_scale(self.scale, self.scale);

        let draw_line_seg = |pixmap: &mut Pixmap, x1: f32, y1: f32, x2: f32, y2: f32| {
            let mut paint = Paint::default();
            paint.set_color(dec_color.to_tiny_skia());
            paint.anti_alias = true;
            let mut stroke = Stroke::default();
            stroke.width = line_thick;
            match dec_style {
                TextDecorationStyle::Solid => {
                    if let Some(path) = line_path(x1, y1, x2, y2) {
                        pixmap.stroke_path(&path, &paint, &stroke, ts, mask);
                    }
                }
                TextDecorationStyle::Double => {
                    if let Some(path) = line_path(x1, y1, x2, y2) {
                        pixmap.stroke_path(&path, &paint, &stroke, ts, mask);
                    }
                    let gap = line_thick + 1.0;
                    if let Some(path) = line_path(x1, y1 + gap, x2, y2 + gap) {
                        pixmap.stroke_path(&path, &paint, &stroke, ts, mask);
                    }
                }
                TextDecorationStyle::Dotted => {
                    draw_dotted_line(pixmap, &paint, line_thick, x1, y1, x2, y2, self.scale);
                }
                TextDecorationStyle::Dashed => {
                    draw_dashed_line(pixmap, &paint, line_thick, x1, y1, x2, y2, self.scale);
                }
                TextDecorationStyle::Wavy => {
                    let amplitude = line_thick.max(1.0);
                    let period = (font_px * 0.4).max(4.0);
                    stroke.width = (line_thick * 0.6).max(0.5);
                    let mut xp = x1;
                    while xp < x2 {
                        let t1 = (xp - x1) / period * std::f32::consts::TAU;
                        let t2 = ((xp + period * 0.25) - x1) / period * std::f32::consts::TAU;
                        let yp1 = y1 + t1.sin() * amplitude;
                        let yp2 = y1 + t2.sin() * amplitude;
                        if let Some(path) = line_path(xp, yp1, (xp + period * 0.25).min(x2), yp2) {
                            pixmap.stroke_path(&path, &paint, &stroke, ts, mask);
                        }
                        xp += period * 0.25;
                    }
                }
            }
        };

        if dec.underline {
            let offset = node.style.text_underline_offset.resolve(font_px, font_px, 16.0);
            let offset = if offset > 0.0 { offset } else { 2.0 };
            let uy = oy + line.ascent + offset;
            draw_line_seg(pixmap, ox, uy, ox + lw, uy);
        }
        if dec.strikethrough {
            let sy2 = oy + line.ascent - font_px * 0.30;
            draw_line_seg(pixmap, ox, sy2, ox + lw, sy2);
        }
        if dec.overline {
            draw_line_seg(pixmap, ox, oy, ox + lw, oy);
        }
    }

    // ─── Text run (cosmic-text) ───────────────────────────────────────────────

    /// Draw a text run using cosmic-text and return the **logical-pixel advance**
    /// (the true rendered width, used by callers to advance the cursor).
    fn draw_text_run(
        &mut self,
        text:        &str,
        x:           f32,
        y:           f32,
        font_px:     f32,
        line_h:      f32,
        weight:      FontWeight,
        font_style:  FontStyle,
        font_family: &str,
        color:       CTextColor,
        pixmap:      &mut Pixmap,
        mask:        Option<&Mask>,
    ) -> f32 {
        self.draw_text_run_ex(text, x, y, font_px, line_h, weight, font_style,
            font_family, 100.0, &[], color, pixmap, mask)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text_run_ex(
        &mut self,
        text:            &str,
        x:               f32,
        y:               f32,
        font_px:         f32,
        line_h:          f32,
        weight:          FontWeight,
        font_style:      FontStyle,
        font_family:     &str,
        font_stretch:    f32,
        variation:       &[(String, f32)],
        color:           CTextColor,
        pixmap:          &mut Pixmap,
        mask:            Option<&Mask>,
    ) -> f32 {
        if text.is_empty() { return 0.0; }
        // Cosmic-text shapes at physical pixel sizes for correct sub-pixel rendering.
        let sc       = self.scale;
        let phys_px  = font_px  * sc;
        let phys_lh  = line_h   * sc;
        let metrics = Metrics::new(phys_px, phys_lh);
        let family  = css_family_to_cosmic(font_family);
        let ct_w    = weight_from_style(weight, variation);
        let ct_s    = match font_style {
            FontStyle::Italic  => CTextStyle::Italic,
            FontStyle::Oblique => CTextStyle::Oblique,
            FontStyle::Normal  => CTextStyle::Normal,
        };
        let ct_stretch = stretch_from_percent(font_stretch);
        let attrs = Attrs::new()
            .weight(ct_w)
            .style(ct_s)
            .stretch(ct_stretch)
            .family(family);

        // Reuse a single Buffer across calls to avoid per-run allocation.
        // take() lets us hold a mutable ref to font_system at the same time.
        if self.shape_buf.is_none() {
            self.shape_buf = Some(Buffer::new(&mut self.font_system, metrics));
        }
        let mut buf = self.shape_buf.take().unwrap();
        buf.set_metrics(&mut self.font_system, metrics);
        buf.set_size(
            &mut self.font_system,
            None,                           // no width constraint — layout already broke lines
            Some((phys_lh + 4.0).max(1.0)),
        );
        // Always use Advanced (HarfBuzz) shaping so that glyph advances match the
        // positions computed by fill_char_x_for_line (which must use Advanced because
        // Shaping::Basic reports word-relative, not buffer-relative, byte offsets).
        // Using different shaping here vs. layout causes kerning differences that
        // shift click-to-caret mapping by 1-3 px per kerned pair.
        let shaping = Shaping::Advanced;
        buf.set_text(&mut self.font_system, text, &attrs, shaping, None);
        buf.shape_until_scroll(&mut self.font_system, false);

        // Measure the actual advance from the shaped run (physical pixels → logical).
        let mut phys_advance = 0.0f32;
        for run in buf.layout_runs() {
            if run.line_w > phys_advance { phys_advance = run.line_w; }
        }
        let logical_advance = phys_advance / sc;

        // Glyph positions from cosmic-text are in physical pixels.
        let phys_x = x * sc;
        let phys_y = y * sc;

        // cosmic-text ignores color.a in its mask glyph callback (//TODO: blend base alpha?
        // comment still present in 0.18.2). Capture it here and multiply with coverage.
        let color_a = color.a() as u32;

        if mask.is_none() {
            // ── Fast path ────────────────────────────────────────────────────
            // Write glyph coverage directly into the pixmap's pixel buffer.
            // This avoids per-pixel fill_rect overhead (~1 μs/call → ~10 ns/pixel),
            // which was the dominant render cost for text-heavy documents.
            let pix_w  = pixmap.width()  as i32;
            let pix_h  = pixmap.height() as i32;
            let stride = pix_w as usize;
            let pixels = pixmap.pixels_mut();
            buf.draw(&mut self.font_system, &mut self.swash_cache, color, |gx, gy, gw, gh, gc| {
                let ga = gc.a();
                if ga == 0 { return; }
                // Apply color.a (dropped by cosmic-text) by multiplying with coverage.
                let eff_a = ga as u32 * color_a / 255;
                if eff_a == 0 { return; }
                let bx = phys_x as i32 + gx;
                let by = phys_y as i32 + gy;
                let sa = eff_a;
                let ia = 255 - sa;
                // Premultiply source color.
                let pr = gc.r() as u32 * sa / 255;
                let pg = gc.g() as u32 * sa / 255;
                let pb = gc.b() as u32 * sa / 255;
                for dy in 0..gh as i32 {
                    let py = by + dy;
                    if py < 0 || py >= pix_h { continue; }
                    let row = py as usize * stride;
                    for dx in 0..gw as i32 {
                        let px = bx + dx;
                        if px < 0 || px >= pix_w { continue; }
                        let dst = &mut pixels[row + px as usize];
                        // Porter-Duff "over" (premultiplied src over premultiplied dst).
                        let r = (pr + dst.red()   as u32 * ia / 255) as u8;
                        let g = (pg + dst.green() as u32 * ia / 255) as u8;
                        let b = (pb + dst.blue()  as u32 * ia / 255) as u8;
                        let a = (sa + dst.alpha() as u32 * ia / 255) as u8;
                        // r <= a is guaranteed by valid premultiplied math, so unwrap is safe.
                        if let Some(p) = tiny_skia::PremultipliedColorU8::from_rgba(r, g, b, a) {
                            *dst = p;
                        }
                    }
                }
            });
        } else {
            // ── Slow path ────────────────────────────────────────────────────
            // Only reached for rounded-corner overflow clips (rare). Uses fill_rect
            // so the pixel-level mask is respected.
            buf.draw(&mut self.font_system, &mut self.swash_cache, color, |gx, gy, gw, gh, gc| {
                let eff_a = (gc.a() as u32 * color_a / 255) as u8;
                if eff_a == 0 { return; }
                if let Some(rect) = SkRect::from_xywh(
                    phys_x + gx as f32, phys_y + gy as f32, gw as f32, gh as f32,
                ) {
                    let mut paint = Paint::default();
                    paint.set_color_rgba8(gc.r(), gc.g(), gc.b(), eff_a);
                    paint.anti_alias = true;
                    pixmap.fill_rect(rect, &paint, Transform::identity(), mask);
                }
            });
        }

        self.shape_buf = Some(buf);
        logical_advance
    }

    // ─── Text measurement ────────────────────────────────────────────────────

    /// Shape `text` and return its logical-pixel advance without drawing.
    fn measure_text_run(&mut self, text: &str, font_px: f32, line_h: f32,
                        weight: FontWeight, font_style: FontStyle) -> f32 {
        if text.is_empty() { return 0.0; }
        let sc      = self.scale;
        let phys_px = font_px * sc;
        let phys_lh = line_h  * sc;
        let metrics = Metrics::new(phys_px, phys_lh);
        let attrs = Attrs::new()
            .weight(if weight.is_bold() { Weight::BOLD } else { Weight::NORMAL })
            .style(match font_style {
                FontStyle::Italic  => CTextStyle::Italic,
                FontStyle::Oblique => CTextStyle::Oblique,
                FontStyle::Normal  => CTextStyle::Normal,
            });
        if self.shape_buf.is_none() {
            self.shape_buf = Some(Buffer::new(&mut self.font_system, metrics));
        }
        let mut buf = self.shape_buf.take().unwrap();
        buf.set_metrics(&mut self.font_system, metrics);
        buf.set_size(&mut self.font_system, None, Some((phys_lh + 4.0).max(1.0)));
        let shaping = if text.is_ascii() { Shaping::Basic } else { Shaping::Advanced };
        buf.set_text(&mut self.font_system, text, &attrs, shaping, None);
        buf.shape_until_scroll(&mut self.font_system, false);
        let mut phys_advance = 0.0f32;
        for run in buf.layout_runs() {
            if run.line_w > phys_advance { phys_advance = run.line_w; }
        }
        self.shape_buf = Some(buf);
        phys_advance / sc
    }

    // ─── List marker ─────────────────────────────────────────────────────────

    fn draw_list_marker(
        &mut self,
        node:   &HtmlBox,
        pixmap: &mut Pixmap,
        sx:     f32,
        sy:     f32,
        mask:   Option<&Mask>,
    ) {
        let ms         = node.style.marker_style.as_deref();
        let font_px    = ms.map(|s| s.font_size_px(16.0, 16.0)).unwrap_or_else(|| node.style.font_size_px(16.0, 16.0));
        let first_line = match node.line_cache.first() { Some(l) => l.clone(), None => return };
        let inside     = node.style.list_style_position == ListStylePosition::Inside;

        let c = ms.map(|s| s.color).unwrap_or(node.style.color);
        let mut paint = Paint::default();
        paint.set_color(c.to_tiny_skia());
        paint.anti_alias = true;

        match node.style.list_style_type {
            ListStyleType::Disc => {
                let bx = if inside { first_line.x - sx + 4.0 } else { first_line.x - sx - 10.0 };
                let by = first_line.y - sy + first_line.height / 2.0;
                if let Some(path) = circle_path(bx, by, 3.0) {
                    pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::from_scale(self.scale, self.scale), mask);
                }
            }
            ListStyleType::Circle => {
                let bx = if inside { first_line.x - sx + 4.0 } else { first_line.x - sx - 10.0 };
                let by = first_line.y - sy + first_line.height / 2.0;
                let mut stroke = Stroke::default();
                stroke.width = 1.0;
                if let Some(path) = circle_path(bx, by, 3.0) {
                    pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), mask);
                }
            }
            ListStyleType::Square => {
                let bx = if inside { first_line.x - sx + 4.0 } else { first_line.x - sx - 10.0 };
                let by = first_line.y - sy + first_line.height / 2.0;
                let s  = 6.0f32;
                if let Some(r) = SkRect::from_xywh(bx - s/2.0, by - s/2.0, s, s) {
                    pixmap.fill_rect(r, &paint, Transform::from_scale(self.scale, self.scale), mask);
                }
            }
            ListStyleType::Decimal | ListStyleType::LowerAlpha | ListStyleType::UpperAlpha
            | ListStyleType::LowerRoman | ListStyleType::UpperRoman => {
                let marker = format_list_marker(node.style.list_style_type, node.style.list_index);
                let line_h = node.style.line_height.resolve(font_px, 0.0, 16.0).max(font_px * 1.2);
                let marker_w = self.measure_text_run(&marker, font_px, line_h,
                    node.style.font_weight, node.style.font_style);
                let mx = if inside { first_line.x - sx } else { first_line.x - sx - marker_w - 4.0 };
                let my = first_line.y - sy;
                let ct_color = CTextColor::rgba(c.r, c.g, c.b, c.a);
                self.draw_text_run(&marker, mx, my, font_px, line_h,
                    node.style.font_weight, node.style.font_style, &node.style.font_family, ct_color, pixmap, mask);
            }
            ListStyleType::Disclosure => {
                let marker = "▸";
                let line_h = node.style.line_height.resolve(font_px, 0.0, 16.0).max(font_px * 1.2);
                let marker_w = self.measure_text_run(marker, font_px, line_h,
                    node.style.font_weight, node.style.font_style);
                let mx = if inside { first_line.x - sx } else { first_line.x - sx - marker_w - 4.0 };
                let my = first_line.y - sy;
                let ct_color = CTextColor::rgba(c.r, c.g, c.b, c.a);
                self.draw_text_run(marker, mx, my, font_px, line_h,
                    node.style.font_weight, node.style.font_style, &node.style.font_family, ct_color, pixmap, mask);
            }
            ListStyleType::None => {}
        }
    }

    // ─── HR ──────────────────────────────────────────────────────────────────

    fn draw_hr(&self, node: &HtmlBox, pixmap: &mut Pixmap, sx: f32, sy: f32, mask: Option<&Mask>) {
        let cr = node.border_rect;
        let y  = cr.y + cr.h / 2.0 - sy;
        let mut paint = Paint::default();
        paint.set_color_rgba8(128, 128, 128, 255);
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        if let Some(path) = line_path(cr.x - sx, y, cr.right() - sx, y) {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), mask);
        }
    }

    // ─── Image drawing ───────────────────────────────────────────────────────

    fn draw_image_placeholder(
        &self,
        node:   &HtmlBox,
        pixmap: &mut Pixmap,
        sx:     f32,
        sy:     f32,
        mask:   Option<&Mask>,
    ) {
        let cr = node.content_rect;
        if cr.w <= 0.0 || cr.h <= 0.0 { return; }

        // If we have actual pixel data, draw it
        if let Some(data) = &node.image_data {
            if node.image_width > 0 && node.image_height > 0 {
                self.draw_image_data(
                    data, node.image_width, node.image_height,
                    cr, node.style.object_fit,
                    pixmap, sx, sy, mask,
                );
                return;
            }
        }

        // Fallback: draw grey placeholder with border
        let rx = cr.x - sx;
        let ry = cr.y - sy;
        let mut paint = Paint::default();
        paint.set_color_rgba8(220, 220, 220, 200);
        if let Some(r) = SkRect::from_xywh(rx, ry, cr.w, cr.h) {
            pixmap.fill_rect(r, &paint, Transform::from_scale(self.scale, self.scale), mask);
        }
        paint.set_color_rgba8(180, 180, 180, 255);
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        if let Some(path) = rect_path(rx, ry, cr.w, cr.h) {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), mask);
        }
    }

    fn draw_image_data(
        &self,
        data:     &[u8],
        img_w:    u32,
        img_h:    u32,
        dest:     Rect,
        fit:      ObjectFit,
        pixmap:   &mut Pixmap,
        sx:       f32,
        sy:       f32,
        mask:     Option<&Mask>,
    ) {
        // Build a tiny_skia Pixmap from RGBA8 data
        // tiny_skia uses premultiplied alpha internally
        let mut src_pm = match Pixmap::new(img_w, img_h) {
            Some(p) => p,
            None => return,
        };
        // Copy, converting straight alpha to premultiplied
        {
            let pix = src_pm.pixels_mut();
            let src_len = (img_w * img_h * 4) as usize;
            if data.len() < src_len { return; }
            for (i, px) in pix.iter_mut().enumerate() {
                let base = i * 4;
                let r = data[base] as u32;
                let g = data[base + 1] as u32;
                let b = data[base + 2] as u32;
                let a = data[base + 3];
                // Premultiply
                let pr = ((r * a as u32 + 127) / 255) as u8;
                let pg = ((g * a as u32 + 127) / 255) as u8;
                let pb = ((b * a as u32 + 127) / 255) as u8;
                *px = tiny_skia::PremultipliedColorU8::from_rgba(pr, pg, pb, a)
                    .unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
            }
        }

        let dest_x = dest.x - sx;
        let dest_y = dest.y - sy;
        let dest_w = dest.w;
        let dest_h = dest.h;

        let (draw_x, draw_y, draw_w, draw_h, clip_to_dest) = compute_object_fit_rect(
            img_w as f32, img_h as f32, dest_w, dest_h, dest_x, dest_y, fit,
        );

        // Build the scale transform: map src (img_w x img_h) → (draw_w x draw_h)
        let scale_x = draw_w / img_w as f32;
        let scale_y = draw_h / img_h as f32;
        let transform = Transform::from_scale(scale_x, scale_y)
            .pre_concat(Transform::from_translate(draw_x / scale_x, draw_y / scale_y));

        // If the image extends outside the dest rect (cover/none), we need a clip mask
        let clip_mask_storage;
        let final_mask: Option<&Mask>;
        if clip_to_dest {
            // Create an intersection mask: dest rect clipped
            let pw = pixmap.width();
            let ph = pixmap.height();
            if let Some(mut combined) = Mask::new(pw, ph) {
                // Fill the dest rect in the mask
                let mut pb = PathBuilder::new();
                pb.move_to(dest_x, dest_y);
                pb.line_to(dest_x + dest_w, dest_y);
                pb.line_to(dest_x + dest_w, dest_y + dest_h);
                pb.line_to(dest_x, dest_y + dest_h);
                pb.close();
                if let Some(clip_path) = pb.finish() {
                    combined.fill_path(&clip_path, FillRule::Winding, true, Transform::from_scale(self.scale, self.scale));
                }
                // Intersect with existing mask if any — tiny-skia 0.11 has no direct
                // intersect_with, so we manually AND each byte of the mask pixels
                if let Some(m) = mask {
                    let src = m.data();
                    let dst = combined.data_mut();
                    for (d, &s) in dst.iter_mut().zip(src.iter()) {
                        *d = ((*d as u16 * s as u16) / 255) as u8;
                    }
                }
                clip_mask_storage = Some(combined);
                final_mask = clip_mask_storage.as_ref();
            } else {
                final_mask = mask;
                clip_mask_storage = None;
            }
        } else {
            final_mask = mask;
            clip_mask_storage = None;
        }
        let _ = clip_mask_storage; // suppress unused warning when mask not stored

        let final_transform = Transform::from_scale(self.scale, self.scale).pre_concat(transform);
        pixmap.draw_pixmap(
            0, 0,
            src_pm.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            final_transform,
            final_mask,
        );
    }

    // ─── Background image ─────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    fn draw_background_image(
        &self,
        data:     &[u8],
        img_w:    u32,
        img_h:    u32,
        style:    &ComputedStyle,
        px: f32, py: f32, pw: f32, ph: f32,
        font_px: f32,
        radius: f32, r_tl: f32, r_tr: f32, r_br: f32, r_bl: f32,
        pixmap:   &mut Pixmap,
        elem_ts:  Transform,
        mask:     Option<&Mask>,
    ) {
        let iw = img_w as f32;
        let ih = img_h as f32;
        if iw <= 0.0 || ih <= 0.0 { return; }

        // Compute drawn image dimensions based on background-size
        let (draw_w, draw_h) = match style.background_size {
            BackgroundSize::Cover => {
                let scale = (pw / iw).max(ph / ih);
                (iw * scale, ih * scale)
            }
            BackgroundSize::Contain => {
                let scale = (pw / iw).min(ph / ih);
                (iw * scale, ih * scale)
            }
            BackgroundSize::Explicit => {
                let w = if style.background_size_w.is_auto() { iw }
                    else { style.background_size_w.resolve(font_px, pw, 16.0) };
                let h = if style.background_size_h.is_auto() { ih }
                    else { style.background_size_h.resolve(font_px, ph, 16.0) };
                (w, h)
            }
            BackgroundSize::Auto => (iw, ih),
        };
        if draw_w <= 0.0 || draw_h <= 0.0 { return; }

        // Compute position
        let pos_x = px + style.background_position_x.resolve(font_px, pw - draw_w, 16.0);
        let pos_y = py + style.background_position_y.resolve(font_px, ph - draw_h, 16.0);

        // Build source pixmap
        let mut src_pm = match Pixmap::new(img_w, img_h) {
            Some(p) => p,
            None => return,
        };
        {
            let pix = src_pm.pixels_mut();
            let src_len = (img_w * img_h * 4) as usize;
            if data.len() < src_len { return; }
            for (i, px_out) in pix.iter_mut().enumerate() {
                let base = i * 4;
                let r = data[base] as u32;
                let g = data[base + 1] as u32;
                let b = data[base + 2] as u32;
                let a = data[base + 3];
                let pr = ((r * a as u32 + 127) / 255) as u8;
                let pg = ((g * a as u32 + 127) / 255) as u8;
                let pb = ((b * a as u32 + 127) / 255) as u8;
                *px_out = tiny_skia::PremultipliedColorU8::from_rgba(pr, pg, pb, a)
                    .unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
            }
        }

        // Build clip mask for the padding box (background-clip: padding-box default)
        let clip_mask_storage;
        let final_mask: Option<&Mask>;
        {
            let pmw = pixmap.width();
            let pmh = pixmap.height();
            if let Some(mut combined) = Mask::new(pmw, pmh) {
                if radius > 0.0 {
                    if let Some(path) = rounded_rect_path_corners(px, py, pw, ph, r_tl, r_tr, r_br, r_bl) {
                        combined.fill_path(&path, FillRule::Winding, true, Transform::from_scale(self.scale, self.scale));
                    }
                } else {
                    let mut pb = PathBuilder::new();
                    pb.move_to(px, py);
                    pb.line_to(px + pw, py);
                    pb.line_to(px + pw, py + ph);
                    pb.line_to(px, py + ph);
                    pb.close();
                    if let Some(clip_path) = pb.finish() {
                        combined.fill_path(&clip_path, FillRule::Winding, true, Transform::from_scale(self.scale, self.scale));
                    }
                }
                if let Some(m) = mask {
                    let src_mask = m.data();
                    let dst = combined.data_mut();
                    for (d, &s) in dst.iter_mut().zip(src_mask.iter()) {
                        *d = ((*d as u16 * s as u16) / 255) as u8;
                    }
                }
                clip_mask_storage = Some(combined);
                final_mask = clip_mask_storage.as_ref();
            } else {
                final_mask = mask;
                clip_mask_storage = None;
            }
        }
        let _ = clip_mask_storage;

        // Determine tiles based on background-repeat
        let repeat_x = matches!(style.background_repeat, BackgroundRepeat::Repeat | BackgroundRepeat::RepeatX);
        let repeat_y = matches!(style.background_repeat, BackgroundRepeat::Repeat | BackgroundRepeat::RepeatY);

        let start_x = if repeat_x {
            let mut sx = pos_x;
            while sx > px { sx -= draw_w; }
            sx
        } else {
            pos_x
        };
        let start_y = if repeat_y {
            let mut sy = pos_y;
            while sy > py { sy -= draw_h; }
            sy
        } else {
            pos_y
        };

        let end_x = if repeat_x { px + pw } else { start_x + draw_w };
        let end_y = if repeat_y { py + ph } else { start_y + draw_h };

        let scale_x = draw_w / iw;
        let scale_y = draw_h / ih;

        let mut tile_y = start_y;
        while tile_y < end_y {
            let mut tile_x = start_x;
            while tile_x < end_x {
                let transform = Transform::from_scale(scale_x, scale_y)
                    .pre_concat(Transform::from_translate(tile_x / scale_x, tile_y / scale_y));
                let final_transform = elem_ts.pre_concat(transform);
                pixmap.draw_pixmap(
                    0, 0,
                    src_pm.as_ref(),
                    &tiny_skia::PixmapPaint::default(),
                    final_transform,
                    final_mask,
                );
                tile_x += draw_w;
                if !repeat_x { break; }
            }
            tile_y += draw_h;
            if !repeat_y { break; }
        }
    }

    // ─── Scrollbars ──────────────────────────────────────────────────────────

    fn draw_scrollbars(&self, node: &HtmlBox, pixmap: &mut Pixmap, sx: f32, sy: f32) {
        let show_v = node.style.overflow_y == Overflow::Scroll
            || (node.style.overflow_y == Overflow::Auto && node.scroll_height > node.content_rect.h);
        let show_h = node.style.overflow_x == Overflow::Scroll
            || (node.style.overflow_x == Overflow::Auto && node.scroll_width > node.content_rect.w);
        if !show_v && !show_h { return; }

        let thumb_col = node.style.scrollbar_thumb_color.unwrap_or(Color::rgba(128, 128, 128, 160));
        let track_col = node.style.scrollbar_track_color.unwrap_or(Color::rgba(128, 128, 128, 40));

        let cr = node.content_rect;
        let pr = node.padding_rect;
        // Screen-space top-left of the padding box (used to anchor the scrollbar
        // at the right/bottom edge of the visible border-box interior).
        let prx = pr.x - sx;
        let pry = pr.y - sy;
        let cy = cr.y - sy;  // content rect top (for track start / scroll math)
        let ts = Transform::from_scale(self.scale, self.scale);

        if show_v && node.scroll_height > cr.h {
            let track_h = cr.h;
            let thumb_h = (track_h * track_h / node.scroll_height).max(20.0);
            let max_s   = node.scroll_height - cr.h;
            let thumb_y = if max_s > 0.0 { node.scroll_top * (track_h - thumb_h) / max_s } else { 0.0 };
            // Align to the right edge of the padding box.
            let track_x = prx + pr.w - SCROLLBAR_WIDTH;
            let mut paint = Paint::default();
            paint.set_color(track_col.to_tiny_skia());
            if let Some(r) = SkRect::from_xywh(track_x, cy, SCROLLBAR_WIDTH, track_h) {
                pixmap.fill_rect(r, &paint, ts, None);
            }
            paint.set_color(thumb_col.to_tiny_skia());
            if let Some(path) = rounded_rect_path(track_x + 1.0, cy + thumb_y + 1.0,
                    SCROLLBAR_WIDTH - 2.0, thumb_h - 2.0, 3.0) {
                pixmap.fill_path(&path, &paint, FillRule::Winding, ts, None);
            }
        }

        if show_h && node.scroll_width > cr.w {
            let track_w = pr.w - if show_v { SCROLLBAR_WIDTH } else { 0.0 };
            let thumb_w = (track_w * track_w / node.scroll_width).max(20.0);
            let max_s   = node.scroll_width - cr.w;
            let thumb_x = if max_s > 0.0 { node.scroll_left * (track_w - thumb_w) / max_s } else { 0.0 };
            // Align to the bottom edge of the padding box.
            let track_y = pry + pr.h - SCROLLBAR_WIDTH;
            let mut paint = Paint::default();
            paint.set_color(track_col.to_tiny_skia());
            if let Some(r) = SkRect::from_xywh(prx, track_y, track_w, SCROLLBAR_WIDTH) {
                pixmap.fill_rect(r, &paint, ts, None);
            }
            paint.set_color(thumb_col.to_tiny_skia());
            if let Some(path) = rounded_rect_path(prx + thumb_x + 1.0, track_y + 1.0,
                    thumb_w - 2.0, SCROLLBAR_WIDTH - 2.0, 3.0) {
                pixmap.fill_path(&path, &paint, FillRule::Winding, ts, None);
            }
        }
    }

    // ─── Caret ───────────────────────────────────────────────────────────────
    // Mirrors C++ Render() caret section: finds the line containing caretPos
    // and draws a vertical line using the box's caret-color / color.

    fn draw_caret(
        &mut self,
        root:         &HtmlBox,
        pixmap:       &mut Pixmap,
        sx:           f32,
        sy:           f32,
        caret_box_ptr: *const HtmlBox,
        caret_local:  usize,
    ) {
        self.draw_caret_walk(root, pixmap, sx, sy, caret_box_ptr, caret_local);
    }

    fn draw_caret_walk(
        &mut self,
        node:          &HtmlBox,
        pixmap:        &mut Pixmap,
        sx:            f32,
        sy:            f32,
        caret_box_ptr: *const HtmlBox,
        caret_local:   usize,
    ) -> bool {
        if std::ptr::eq(node as *const HtmlBox, caret_box_ptr) {
            // Found the box; find its line
            let flat    = collect_flat_text(node);
            let font_px = node.style.font_size_px(16.0, 16.0);

            let mut caret_x    = node.border_rect.x - sx;
            let mut caret_y    = node.border_rect.y - sy;
            let mut caret_h    = font_px * 1.2;
            let mut found_line = false;

            // When caret_local sits at the boundary between two lines
            // (end of line N == start of line N+1), prefer the line where
            // caret_local == line.text_start (i.e. the caret is at the
            // *beginning* of that line).  This is the correct position after
            // pressing Enter or being at the start of a wrapped/br line.
            for line in &node.line_cache {
                let line_end = line.text_start + line.text_length;
                if caret_local >= line.text_start && caret_local <= line_end {
                    caret_y = line.y - sy;
                    caret_h = line.height.max(font_px * 1.0);
                    // Use the same measurement as the hit test (get_caret_x) so
                    // that click position and rendered caret position agree.
                    let cx = crate::layout::hit_test::get_caret_x(
                        &flat, &node.inline_runs, line, caret_local,
                    );
                    caret_x = cx - sx;
                    found_line = true;
                    // A line-start match is unambiguous — no need to search further.
                    if caret_local == line.text_start {
                        break;
                    }
                    // Otherwise keep going: a later line might have text_start ==
                    // caret_local (the current match was an end-of-previous-line).
                }
            }
            if !found_line && !node.line_cache.is_empty() {
                let last = node.line_cache.last().unwrap();
                caret_y = last.y - sy;
                caret_h = last.height.max(font_px);
                caret_x = last.x - sx + last.width;
            }

            // Resolve caret color: caret-color > color > black
            let col = node.style.caret_color
                .unwrap_or(node.style.color);

            let mut paint = Paint::default();
            paint.set_color(col.to_tiny_skia());
            let mut stroke = Stroke::default();
            stroke.width = 1.5;
            if let Some(path) = line_path(caret_x, caret_y, caret_x, caret_y + caret_h) {
                pixmap.stroke_path(&path, &paint, &stroke, Transform::from_scale(self.scale, self.scale), None);
            }
            return true;
        }

        for child in &node.children {
            if self.draw_caret_walk(child, pixmap, sx, sy, caret_box_ptr, caret_local) {
                return true;
            }
        }
        false
    }
}

// ─── Clip-path mask builder ───────────────────────────────────────────────────

fn make_clip_path_mask(
    pixmap: &Pixmap,
    node:   &HtmlBox,
    px: f32, py: f32, pw: f32, ph: f32,
    font_px: f32,
    scale: f32,
) -> Option<Mask> {
    let cp = &node.style.clip_path;
    if cp.kind == ClipPathKind::None || pw <= 0.0 || ph <= 0.0 { return None; }

    let path = match cp.kind {
        ClipPathKind::Inset => {
            let t = cp.inset_top.resolve(font_px, ph, 16.0);
            let r = cp.inset_right.resolve(font_px, pw, 16.0);
            let b = cp.inset_bottom.resolve(font_px, ph, 16.0);
            let l = cp.inset_left.resolve(font_px, pw, 16.0);
            rect_path(px + l, py + t, pw - l - r, ph - t - b)?
        }
        ClipPathKind::Circle => {
            let cx  = cp.center_x.resolve(font_px, pw, 16.0) + px;
            let cy  = cp.center_y.resolve(font_px, ph, 16.0) + py;
            let ref_r = (pw * pw + ph * ph).sqrt() / std::f32::consts::SQRT_2;
            let r   = cp.circle_radius.resolve(font_px, ref_r, 16.0);
            circle_path(cx, cy, r)?
        }
        ClipPathKind::Ellipse => {
            let cx = cp.center_x.resolve(font_px, pw, 16.0) + px;
            let cy = cp.center_y.resolve(font_px, ph, 16.0) + py;
            let rx = cp.ellipse_rx.resolve(font_px, pw, 16.0);
            let ry = cp.ellipse_ry.resolve(font_px, ph, 16.0);
            ellipse_path(cx, cy, rx, ry)?
        }
        ClipPathKind::Polygon => {
            if cp.points.len() < 3 { return None; }
            polygon_path(&cp.points, px, py, pw, ph, font_px)?
        }
        ClipPathKind::None => return None,
    };

    let ts = Transform::from_scale(scale, scale);
    let mut mask = Mask::new(pixmap.width(), pixmap.height())?;
    mask.fill_path(&path, FillRule::Winding, true, ts);
    Some(mask)
}

// ─── Overflow clip mask builder ───────────────────────────────────────────────

fn make_overflow_clip_mask(
    pixmap: &Pixmap,
    px: f32, py: f32, pw: f32, ph: f32,
    radius: f32,
    scale: f32,
) -> Option<Mask> {
    if pw <= 0.0 || ph <= 0.0 { return None; }
    // For rectangular clips (no border-radius) we skip the full-viewport Mask allocation
    // (~2-10 MB per element) and rely on the child_clip rect for culling instead.
    // Only rounded corners genuinely need a pixel-level mask.
    if radius <= 0.0 { return None; }
    let path = rounded_rect_path(px, py, pw, ph, radius)?;
    let ts = Transform::from_scale(scale, scale);
    let mut mask = Mask::new(pixmap.width(), pixmap.height())?;
    mask.fill_path(&path, FillRule::Winding, true, ts);
    Some(mask)
}

// ─── Object-fit rect computation ──────────────────────────────────────────────

/// Returns (draw_x, draw_y, draw_w, draw_h, clip_to_dest).
/// draw_* are the final screen coordinates to draw the image at (possibly larger than dest).
/// clip_to_dest = true means the image must be clipped to dest bounds.
fn compute_object_fit_rect(
    img_w: f32, img_h: f32,
    dest_w: f32, dest_h: f32,
    dest_x: f32, dest_y: f32,
    fit: ObjectFit,
) -> (f32, f32, f32, f32, bool) {
    match fit {
        ObjectFit::Fill => {
            (dest_x, dest_y, dest_w, dest_h, false)
        }
        ObjectFit::Contain => {
            let scale = (dest_w / img_w).min(dest_h / img_h);
            let dw = img_w * scale;
            let dh = img_h * scale;
            let dx = dest_x + (dest_w - dw) / 2.0;
            let dy = dest_y + (dest_h - dh) / 2.0;
            (dx, dy, dw, dh, false)
        }
        ObjectFit::Cover => {
            let scale = (dest_w / img_w).max(dest_h / img_h);
            let dw = img_w * scale;
            let dh = img_h * scale;
            let dx = dest_x + (dest_w - dw) / 2.0;
            let dy = dest_y + (dest_h - dh) / 2.0;
            (dx, dy, dw, dh, true)
        }
        ObjectFit::None => {
            // Natural size, centered, clipped
            let dx = dest_x + (dest_w - img_w) / 2.0;
            let dy = dest_y + (dest_h - img_h) / 2.0;
            (dx, dy, img_w, img_h, true)
        }
        ObjectFit::ScaleDown => {
            // Smaller of contain vs none
            let scale = ((dest_w / img_w).min(dest_h / img_h)).min(1.0);
            let dw = img_w * scale;
            let dh = img_h * scale;
            let dx = dest_x + (dest_w - dw) / 2.0;
            let dy = dest_y + (dest_h - dh) / 2.0;
            (dx, dy, dw, dh, false)
        }
    }
}

// ─── Path helpers ─────────────────────────────────────────────────────────────

fn rect_path(x: f32, y: f32, w: f32, h: f32) -> Option<tiny_skia::Path> {
    if w <= 0.0 || h <= 0.0 { return None; }
    let mut pb = PathBuilder::new();
    pb.move_to(x, y);
    pb.line_to(x + w, y);
    pb.line_to(x + w, y + h);
    pb.line_to(x, y + h);
    pb.close();
    pb.finish()
}

fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    rounded_rect_path_corners(x, y, w, h, r, r, r, r)
}

fn rounded_rect_path_corners(x: f32, y: f32, w: f32, h: f32,
    tl: f32, tr: f32, br: f32, bl: f32,
) -> Option<tiny_skia::Path> {
    let max_r = (w / 2.0).min(h / 2.0);
    let tl = tl.min(max_r);
    let tr = tr.min(max_r);
    let br = br.min(max_r);
    let bl = bl.min(max_r);
    if tl <= 0.0 && tr <= 0.0 && br <= 0.0 && bl <= 0.0 {
        return rect_path(x, y, w, h);
    }
    let k = 0.5522848_f32;  // kappa for quarter-circle approximation
    let mut pb = PathBuilder::new();
    // Top edge
    pb.move_to(x + tl, y);
    pb.line_to(x + w - tr, y);
    // Top-right corner
    if tr > 0.0 {
        pb.cubic_to(x + w - tr + tr*k, y,  x + w, y + tr - tr*k,  x + w, y + tr);
    }
    // Right edge
    pb.line_to(x + w, y + h - br);
    // Bottom-right corner
    if br > 0.0 {
        pb.cubic_to(x + w, y + h - br + br*k,  x + w - br + br*k, y + h,  x + w - br, y + h);
    }
    // Bottom edge
    pb.line_to(x + bl, y + h);
    // Bottom-left corner
    if bl > 0.0 {
        pb.cubic_to(x + bl - bl*k, y + h,  x, y + h - bl + bl*k,  x, y + h - bl);
    }
    // Left edge
    pb.line_to(x, y + tl);
    // Top-left corner
    if tl > 0.0 {
        pb.cubic_to(x, y + tl - tl*k,  x + tl - tl*k, y,  x + tl, y);
    }
    pb.close();
    pb.finish()
}

fn line_path(x1: f32, y1: f32, x2: f32, y2: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(x1, y1);
    pb.line_to(x2, y2);
    pb.finish()
}

fn circle_path(cx: f32, cy: f32, r: f32) -> Option<tiny_skia::Path> {
    let k = 0.5522848f32;
    let mut pb = PathBuilder::new();
    pb.move_to(cx, cy - r);
    pb.cubic_to(cx + r*k, cy - r,   cx + r,   cy - r*k,  cx + r, cy);
    pb.cubic_to(cx + r,   cy + r*k, cx + r*k, cy + r,    cx,     cy + r);
    pb.cubic_to(cx - r*k, cy + r,   cx - r,   cy + r*k,  cx - r, cy);
    pb.cubic_to(cx - r,   cy - r*k, cx - r*k, cy - r,    cx,     cy - r);
    pb.close();
    pb.finish()
}

fn ellipse_path(cx: f32, cy: f32, rx: f32, ry: f32) -> Option<tiny_skia::Path> {
    let k = 0.5522848f32;
    let mut pb = PathBuilder::new();
    pb.move_to(cx, cy - ry);
    pb.cubic_to(cx + rx*k, cy - ry, cx + rx, cy - ry*k, cx + rx, cy);
    pb.cubic_to(cx + rx, cy + ry*k, cx + rx*k, cy + ry, cx, cy + ry);
    pb.cubic_to(cx - rx*k, cy + ry, cx - rx, cy + ry*k, cx - rx, cy);
    pb.cubic_to(cx - rx, cy - ry*k, cx - rx*k, cy - ry, cx, cy - ry);
    pb.close();
    pb.finish()
}

fn polygon_path(
    points: &[(CssLength, CssLength)],
    px: f32, py: f32, pw: f32, ph: f32,
    font_px: f32,
) -> Option<tiny_skia::Path> {
    if points.len() < 3 { return None; }
    let mut pb = PathBuilder::new();
    let (x0, y0) = (
        points[0].0.resolve(font_px, pw, 16.0) + px,
        points[0].1.resolve(font_px, ph, 16.0) + py,
    );
    pb.move_to(x0, y0);
    for pt in &points[1..] {
        let vx = pt.0.resolve(font_px, pw, 16.0) + px;
        let vy = pt.1.resolve(font_px, ph, 16.0) + py;
        pb.line_to(vx, vy);
    }
    pb.close();
    pb.finish()
}

fn draw_dashed_line(pixmap: &mut Pixmap, paint: &Paint, w: f32, x1: f32, y1: f32, x2: f32, y2: f32, scale: f32) {
    let dash_len = w * 3.0;
    let gap_len  = w * 2.0;
    let dx = x2 - x1; let dy = y2 - y1;
    let len = (dx*dx + dy*dy).sqrt();
    if len < 0.5 { return; }
    let nx = dx / len; let ny = dy / len;
    let mut t = 0.0f32; let mut on = true;
    let mut stroke = Stroke::default();
    stroke.width = w;
    while t < len {
        let seg = if on { dash_len } else { gap_len };
        if on {
            let ex = (t + seg).min(len);
            if let Some(path) = line_path(x1 + nx*t, y1 + ny*t, x1 + nx*ex, y1 + ny*ex) {
                pixmap.stroke_path(&path, paint, &stroke, Transform::from_scale(scale, scale), None);
            }
        }
        t += seg; on = !on;
    }
}

fn draw_dotted_line(pixmap: &mut Pixmap, paint: &Paint, w: f32, x1: f32, y1: f32, x2: f32, y2: f32, scale: f32) {
    let r = w / 2.0; let gap = w;
    let dx = x2 - x1; let dy = y2 - y1;
    let len = (dx*dx + dy*dy).sqrt();
    if len < 0.5 { return; }
    let nx = dx / len; let ny = dy / len;
    let mut t = r;
    while t < len {
        if let Some(path) = circle_path(x1 + nx*t, y1 + ny*t, r) {
            pixmap.fill_path(&path, paint, FillRule::Winding, Transform::from_scale(scale, scale), None);
        }
        t += w + gap;
    }
}

// ─── Text helpers ─────────────────────────────────────────────────────────────

fn apply_text_transform(text: &str, tt: TextTransform) -> String {
    match tt {
        TextTransform::Uppercase  => text.to_uppercase(),
        TextTransform::Lowercase  => text.to_lowercase(),
        TextTransform::Capitalize => capitalize_words(text),
        TextTransform::None       => text.to_owned(),
    }
}

fn floor_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() { return s.len(); }
    while idx > 0 && !s.is_char_boundary(idx) { idx -= 1; }
    idx
}

fn format_list_marker(lst: ListStyleType, index: i32) -> String {
    match lst {
        ListStyleType::Decimal    => format!("{}.", index),
        ListStyleType::LowerAlpha => format!("{}.", to_alpha(index, false)),
        ListStyleType::UpperAlpha => format!("{}.", to_alpha(index, true)),
        ListStyleType::LowerRoman => format!("{}.", to_roman(index, false)),
        ListStyleType::UpperRoman => format!("{}.", to_roman(index, true)),
        _ => String::from("•"),
    }
}

fn to_alpha(mut n: i32, upper: bool) -> String {
    if n <= 0 { return String::from("?"); }
    let base: u8 = if upper { b'A' } else { b'a' };
    let mut s = String::new();
    while n > 0 {
        n -= 1;
        s.insert(0, (base + (n % 26) as u8) as char);
        n /= 26;
    }
    s
}

fn to_roman(n: i32, upper: bool) -> String {
    let vals = [(1000,"m"),(900,"cm"),(500,"d"),(400,"cd"),
                (100,"c"),(90,"xc"),(50,"l"),(40,"xl"),
                (10,"x"),(9,"ix"),(5,"v"),(4,"iv"),(1,"i")];
    let mut out = String::new(); let mut rem = n;
    for (v, s) in &vals { while rem >= *v { out.push_str(s); rem -= v; } }
    if upper { out.to_ascii_uppercase() } else { out }
}

fn capitalize_words(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_space = true;
    for ch in s.chars() {
        if ch.is_whitespace() { result.push(ch); prev_space = true; }
        else if prev_space    { result.extend(ch.to_uppercase()); prev_space = false; }
        else                  { result.push(ch); }
    }
    result
}

/// Approximate text width with letter-spacing.
fn approx_text_width_ls(text: &str, font_px: f32, letter_spacing: f32) -> f32 {
    let base = font_px * 0.55;
    let mut w = 0.0f32;
    for ch in text.chars() {
        let cw = if "iIlj1!|:;,.'`".contains(ch) { base * 0.45 }
                 else if "mwMW".contains(ch)       { base * 1.20 }
                 else if ch == ' '                  { base * 0.35 }
                 else if ch.is_ascii()              { base }
                 else                               { font_px * 1.0 };  // emoji / CJK: full square
        w += cw + letter_spacing;
    }
    w
}

fn truncate_with_ellipsis(text: &str, font_px: f32, letter_spacing: f32, max_w: f32) -> String {
    let ellipsis = "…";
    let ew = approx_text_width_ls(ellipsis, font_px, letter_spacing);
    let available = max_w - ew;
    if available <= 0.0 { return ellipsis.to_owned(); }
    let mut w = 0.0f32;
    let mut cut = text.len();
    for (i, ch) in text.char_indices() {
        let base = font_px * 0.55;
        let cw = if "iIlj1!|:;,.'`".contains(ch) { base * 0.45 }
                 else if "mwMW".contains(ch)       { base * 1.20 }
                 else if ch == ' '                  { base * 0.35 }
                 else                               { base };
        if w + cw > available { cut = i; break; }
        w += cw + letter_spacing;
    }
    let mut result = text[..cut].to_owned();
    result.push_str(ellipsis);
    result
}

enum Side { Top, Right, Bottom, Left }

impl Default for Renderer {
    fn default() -> Self { Self::new() }
}

// ─── CSS Transform helpers ────────────────────────────────────────────────────

/// Build a tiny_skia Transform from a CssTransform, incorporating origin offset.
/// `ox`, `oy` are the transform origin in logical pixels (screen coords after scroll).
/// Returns a transform in logical pixels, which can then be combined with the scale factor.
fn build_css_transform(
    css: &CssTransform,
    ox: f32, oy: f32,
) -> Transform {
    if css.ops.is_empty() { return Transform::identity(); }

    // Step 1: translate to origin
    // Step 2: apply ops (left to right)
    // Step 3: translate back from origin
    // Combined matrix accumulation
    let mut m = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0]; // [a, b, c, d, e, f] = [sx, kx, ky, sy, tx, ty]

    for op in &css.ops {
        let op_m: [f32; 6] = match op {
            TransformOp::Translate(tx, ty)    => [1.0, 0.0, 0.0, 1.0, *tx, *ty],
            TransformOp::TranslateX(tx)       => [1.0, 0.0, 0.0, 1.0, *tx, 0.0],
            TransformOp::TranslateY(ty)       => [1.0, 0.0, 0.0, 1.0, 0.0, *ty],
            TransformOp::Scale(sx, sy)        => [*sx, 0.0, 0.0, *sy, 0.0, 0.0],
            TransformOp::ScaleX(sx)           => [*sx, 0.0, 0.0, 1.0, 0.0, 0.0],
            TransformOp::ScaleY(sy)           => [1.0, 0.0, 0.0, *sy, 0.0, 0.0],
            TransformOp::Rotate(deg) => {
                let r = deg * std::f32::consts::PI / 180.0;
                let c = r.cos(); let s = r.sin();
                [c, s, -s, c, 0.0, 0.0]
            }
            TransformOp::SkewX(deg) => {
                let t = (deg * std::f32::consts::PI / 180.0).tan();
                [1.0, 0.0, t, 1.0, 0.0, 0.0]
            }
            TransformOp::SkewY(deg) => {
                let t = (deg * std::f32::consts::PI / 180.0).tan();
                [1.0, t, 0.0, 1.0, 0.0, 0.0]
            }
            TransformOp::Matrix(a, b, c, d, e, f) => [*a, *b, *c, *d, *e, *f],
        };
        // Multiply m = m * op_m  (column-major 2D affine: [sx,kx,ky,sy,tx,ty])
        // A = [a c e]   B = [a' c' e']
        //     [b d f]       [b' d' f']
        //     [0 0 1]       [0  0  1 ]
        // C = A*B
        let (a, b, c, d, e, f)   = (m[0], m[1], m[2], m[3], m[4], m[5]);
        let (a2,b2,c2,d2,e2,f2) = (op_m[0], op_m[1], op_m[2], op_m[3], op_m[4], op_m[5]);
        m = [
            a*a2 + c*b2,   b*a2 + d*b2,
            a*c2 + c*d2,   b*c2 + d*d2,
            a*e2 + c*f2 + e,  b*e2 + d*f2 + f,
        ];
    }

    // Apply origin: translate to origin, apply m, translate back
    // T(ox,oy) * M * T(-ox,-oy)
    // The full transform in homogeneous coords:
    // tx' = m[0]*(-ox) + m[2]*(-oy) + m[4] + ox
    // ty' = m[1]*(-ox) + m[3]*(-oy) + m[5] + oy
    let (a, b, c, d, e, f) = (m[0], m[1], m[2], m[3], m[4], m[5]);
    let tx = -a*ox - c*oy + e + ox;
    let ty = -b*ox - d*oy + f + oy;

    // tiny_skia Transform::from_row(sx, ky, kx, sy, tx, ty)
    // Our matrix: [a=sx, b=kx_row(ky_col?), c=ky_row, d=sy, e=tx, f=ty]
    // tiny_skia uses column-major: from_row(a, b, c, d, e, f) maps to
    // the matrix [a c e; b d f; 0 0 1]
    // We want: x' = a*x + c*y + e, y' = b*x + d*y + f
    // That matches our [sx,kx,ky,sy,tx,ty] convention where:
    //   x' = sx*x + ky*y + tx   → a=sx, c=ky, e=tx
    //   y' = kx*x + sy*y + ty   → b=kx, d=sy, f=ty
    // Our m[] = [sx, kx, ky, sy, tx, ty] → a=m[0], b=m[1], c=m[2], d=m[3]
    Transform::from_row(a, b, c, d, tx, ty)
}

/// Map CSS MixBlendMode to tiny_skia::BlendMode.
fn css_blend_mode(mode: MixBlendMode) -> tiny_skia::BlendMode {
    match mode {
        MixBlendMode::Normal     => tiny_skia::BlendMode::SourceOver,
        MixBlendMode::Multiply   => tiny_skia::BlendMode::Multiply,
        MixBlendMode::Screen     => tiny_skia::BlendMode::Screen,
        MixBlendMode::Overlay    => tiny_skia::BlendMode::Overlay,
        MixBlendMode::Darken     => tiny_skia::BlendMode::Darken,
        MixBlendMode::Lighten    => tiny_skia::BlendMode::Lighten,
        MixBlendMode::ColorDodge => tiny_skia::BlendMode::ColorDodge,
        MixBlendMode::ColorBurn  => tiny_skia::BlendMode::ColorBurn,
        MixBlendMode::HardLight  => tiny_skia::BlendMode::HardLight,
        MixBlendMode::SoftLight  => tiny_skia::BlendMode::SoftLight,
        MixBlendMode::Difference => tiny_skia::BlendMode::Difference,
        MixBlendMode::Exclusion  => tiny_skia::BlendMode::Exclusion,
        MixBlendMode::Hue        => tiny_skia::BlendMode::Hue,
        MixBlendMode::Saturation => tiny_skia::BlendMode::Saturation,
        MixBlendMode::Color      => tiny_skia::BlendMode::Color,
        MixBlendMode::Luminosity => tiny_skia::BlendMode::Luminosity,
    }
}

// ─── CSS Filter application ───────────────────────────────────────────────────

/// Apply CSS filter operations to a rectangular region of the pixmap.
/// `rx`, `ry` are in logical pixels. `scale` converts to physical pixels.
/// `radius` is the border-radius in logical pixels; corners outside it are skipped.
fn apply_css_filters(
    pixmap: &mut Pixmap,
    filters: &CssFilters,
    rx: f32, ry: f32, rw: f32, rh: f32,
    radius: f32,
    scale: f32,
) {
    if filters.ops.is_empty() || rw <= 0.0 || rh <= 0.0 { return; }
    let pix_x = (rx * scale) as i32;
    let pix_y = (ry * scale) as i32;
    let pix_w = (rw * scale) as i32;
    let pix_h = (rh * scale) as i32;
    let pw = pixmap.width() as i32;
    let ph = pixmap.height() as i32;

    // Clamp to pixmap bounds
    let x0 = pix_x.max(0);
    let y0 = pix_y.max(0);
    let x1 = (pix_x + pix_w).min(pw);
    let y1 = (pix_y + pix_h).min(ph);
    if x0 >= x1 || y0 >= y1 { return; }
    let region_w = (x1 - x0) as usize;
    let region_h = (y1 - y0) as usize;

    // Pre-compute rounded-rect corner radius in physical pixels.
    let r_px = (radius * scale).min(pix_w as f32 / 2.0).min(pix_h as f32 / 2.0);
    let in_rounded_rect = |screen_x: i32, screen_y: i32| -> bool {
        if r_px <= 0.0 { return true; }
        // Coords relative to element's physical top-left
        let lx = (screen_x - pix_x) as f32 + 0.5;
        let ly = (screen_y - pix_y) as f32 + 0.5;
        let w = pix_w as f32;
        let h = pix_h as f32;
        let near_left   = lx < r_px;
        let near_right  = lx > w - r_px;
        let near_top    = ly < r_px;
        let near_bottom = ly > h - r_px;
        if near_left  && near_top    { let dx = lx - r_px; let dy = ly - r_px; return dx*dx+dy*dy <= r_px*r_px; }
        if near_right && near_top    { let dx = lx-(w-r_px); let dy = ly-r_px; return dx*dx+dy*dy <= r_px*r_px; }
        if near_left  && near_bottom { let dx = lx-r_px; let dy = ly-(h-r_px); return dx*dx+dy*dy <= r_px*r_px; }
        if near_right && near_bottom { let dx = lx-(w-r_px); let dy = ly-(h-r_px); return dx*dx+dy*dy <= r_px*r_px; }
        true
    };

    for filter_op in &filters.ops {
        match filter_op {
            FilterOp::Blur(blur_px) => {
                let radius = ((*blur_px * scale) as i32).max(1).min(32);
                apply_box_blur(pixmap, pw as usize, ph as usize, x0, y0, x1, y1, radius);
            }
            FilterOp::Brightness(f) => {
                let f = *f;
                apply_pixel_op(pixmap, pw, x0, y0, x1, y1, |x, y, r, g, b, a| {
                    if !in_rounded_rect(x, y) { return (r, g, b, a); }
                    let r2 = ((r as f32 * f).min(255.0)) as u8;
                    let g2 = ((g as f32 * f).min(255.0)) as u8;
                    let b2 = ((b as f32 * f).min(255.0)) as u8;
                    (r2, g2, b2, a)
                });
            }
            FilterOp::Contrast(f) => {
                let f = *f;
                apply_pixel_op(pixmap, pw, x0, y0, x1, y1, |x, y, r, g, b, a| {
                    if !in_rounded_rect(x, y) { return (r, g, b, a); }
                    let adj = |c: u8| -> u8 {
                        let c2 = (c as f32 - 128.0) * f + 128.0;
                        c2.max(0.0).min(255.0) as u8
                    };
                    (adj(r), adj(g), adj(b), a)
                });
            }
            FilterOp::Grayscale(f) => {
                let f = *f;
                apply_pixel_op(pixmap, pw, x0, y0, x1, y1, |x, y, r, g, b, a| {
                    if !in_rounded_rect(x, y) { return (r, g, b, a); }
                    let lum = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
                    let r2 = lerp_u8(r, lum as u8, f);
                    let g2 = lerp_u8(g, lum as u8, f);
                    let b2 = lerp_u8(b, lum as u8, f);
                    (r2, g2, b2, a)
                });
            }
            FilterOp::HueRotate(deg) => {
                let deg = *deg;
                apply_pixel_op(pixmap, pw, x0, y0, x1, y1, |x, y, r, g, b, a| {
                    if !in_rounded_rect(x, y) { return (r, g, b, a); }
                    let (h, s, l) = rgb_to_hsl(r, g, b);
                    let h2 = (h + deg / 360.0).rem_euclid(1.0);
                    let (r2, g2, b2) = hsl_to_rgb(h2, s, l);
                    (r2, g2, b2, a)
                });
            }
            FilterOp::Invert(f) => {
                let f = *f;
                apply_pixel_op(pixmap, pw, x0, y0, x1, y1, |x, y, r, g, b, a| {
                    if !in_rounded_rect(x, y) { return (r, g, b, a); }
                    let r2 = lerp_u8(r, 255 - r, f);
                    let g2 = lerp_u8(g, 255 - g, f);
                    let b2 = lerp_u8(b, 255 - b, f);
                    (r2, g2, b2, a)
                });
            }
            FilterOp::Opacity(f) => {
                let f = *f;
                apply_pixel_op(pixmap, pw, x0, y0, x1, y1, |x, y, r, g, b, a| {
                    if !in_rounded_rect(x, y) { return (r, g, b, a); }
                    (r, g, b, ((a as f32) * f) as u8)
                });
            }
            FilterOp::Saturate(f) => {
                let f = *f;
                apply_pixel_op(pixmap, pw, x0, y0, x1, y1, |x, y, r, g, b, a| {
                    if !in_rounded_rect(x, y) { return (r, g, b, a); }
                    let (h, s, l) = rgb_to_hsl(r, g, b);
                    let s2 = (s * f).min(1.0);
                    let (r2, g2, b2) = hsl_to_rgb(h, s2, l);
                    (r2, g2, b2, a)
                });
            }
            FilterOp::Sepia(f) => {
                let f = *f;
                apply_pixel_op(pixmap, pw, x0, y0, x1, y1, |x, y, r, g, b, a| {
                    if !in_rounded_rect(x, y) { return (r, g, b, a); }
                    let rf = r as f32; let gf = g as f32; let bf = b as f32;
                    let sr = (rf * 0.393 + gf * 0.769 + bf * 0.189).min(255.0) as u8;
                    let sg = (rf * 0.349 + gf * 0.686 + bf * 0.168).min(255.0) as u8;
                    let sb = (rf * 0.272 + gf * 0.534 + bf * 0.131).min(255.0) as u8;
                    (lerp_u8(r, sr, f), lerp_u8(g, sg, f), lerp_u8(b, sb, f), a)
                });
            }
            FilterOp::DropShadow { dx, dy, blur, color } => {
                // Drop shadow: draw colored, blurred, offset copy beneath the element.
                // Simple approximation: draw a solid rect offset by dx,dy in shadow color.
                // This is a simplified version since pixel-accurate drop shadow needs a 2-pass approach.
                let _ = (dx, dy, blur, color, region_w, region_h);
                // Skip complex drop-shadow for now (would need temp pixmap)
            }
        }
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t) as u8
}

fn apply_pixel_op<F>(pixmap: &mut Pixmap, pw: i32, x0: i32, y0: i32, x1: i32, y1: i32, mut f: F)
where F: FnMut(i32, i32, u8, u8, u8, u8) -> (u8, u8, u8, u8)
{
    let data = pixmap.data_mut();
    for y in y0..y1 {
        for x in x0..x1 {
            let idx = ((y * pw + x) * 4) as usize;
            if idx + 3 >= data.len() { continue; }
            let (r, g, b, a) = (data[idx], data[idx+1], data[idx+2], data[idx+3]);
            let (r2, g2, b2, a2) = f(x, y, r, g, b, a);
            data[idx] = r2; data[idx+1] = g2; data[idx+2] = b2; data[idx+3] = a2;
        }
    }
}

fn apply_box_blur(
    pixmap: &mut Pixmap,
    pw: usize, ph: usize,
    x0: i32, y0: i32, x1: i32, y1: i32,
    radius: i32,
) {
    // Two-pass separable box blur on premultiplied RGBA data
    let w = pw as i32; let h = ph as i32;
    let data = pixmap.data_mut();
    let len = data.len();

    // Horizontal pass
    let mut row_buf = vec![0u8; (x1 - x0) as usize * 4];
    for y in y0..y1 {
        let yi = y as usize;
        for xi in x0..x1 {
            let mut r = 0i32; let mut g = 0i32; let mut b = 0i32; let mut a = 0i32;
            let mut count = 0i32;
            for dx in -radius..=radius {
                let sx = xi + dx;
                if sx < 0 || sx >= w { continue; }
                let idx = ((yi as i32 * w + sx) * 4) as usize;
                if idx + 3 >= len { continue; }
                r += data[idx] as i32;
                g += data[idx+1] as i32;
                b += data[idx+2] as i32;
                a += data[idx+3] as i32;
                count += 1;
            }
            let bi = ((xi - x0) * 4) as usize;
            if count > 0 {
                row_buf[bi]   = (r / count) as u8;
                row_buf[bi+1] = (g / count) as u8;
                row_buf[bi+2] = (b / count) as u8;
                row_buf[bi+3] = (a / count) as u8;
            }
        }
        for xi in x0..x1 {
            let idx = ((y * w + xi) * 4) as usize;
            if idx + 3 >= len { continue; }
            let bi = ((xi - x0) * 4) as usize;
            data[idx]   = row_buf[bi];
            data[idx+1] = row_buf[bi+1];
            data[idx+2] = row_buf[bi+2];
            data[idx+3] = row_buf[bi+3];
        }
    }

    // Vertical pass
    let mut col_buf = vec![0u8; (y1 - y0) as usize * 4];
    for x in x0..x1 {
        {
            let data = pixmap.data_mut();
            for yi in y0..y1 {
                let mut r = 0i32; let mut g = 0i32; let mut b = 0i32; let mut a = 0i32;
                let mut count = 0i32;
                for dy in -radius..=radius {
                    let sy = yi + dy;
                    if sy < 0 || sy >= h as i32 { continue; }
                    let idx = ((sy * w as i32 + x) * 4) as usize;
                    if idx + 3 >= len { continue; }
                    r += data[idx] as i32;
                    g += data[idx+1] as i32;
                    b += data[idx+2] as i32;
                    a += data[idx+3] as i32;
                    count += 1;
                }
                let bi = ((yi - y0) * 4) as usize;
                if count > 0 {
                    col_buf[bi]   = (r / count) as u8;
                    col_buf[bi+1] = (g / count) as u8;
                    col_buf[bi+2] = (b / count) as u8;
                    col_buf[bi+3] = (a / count) as u8;
                }
            }
        }
        let data = pixmap.data_mut();
        for yi in y0..y1 {
            let idx = ((yi * w as i32 + x) * 4) as usize;
            if idx + 3 >= len { continue; }
            let bi = ((yi - y0) * 4) as usize;
            data[idx]   = col_buf[bi];
            data[idx+1] = col_buf[bi+1];
            data[idx+2] = col_buf[bi+2];
            data[idx+3] = col_buf[bi+3];
        }
    }
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min) < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s < 1e-6 {
        let v = (l * 255.0) as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue_to_rgb = |p: f32, q: f32, mut t: f32| -> f32 {
        if t < 0.0 { t += 1.0; }
        if t > 1.0 { t -= 1.0; }
        if t < 1.0/6.0 { return p + (q - p) * 6.0 * t; }
        if t < 1.0/2.0 { return q; }
        if t < 2.0/3.0 { return p + (q - p) * (2.0/3.0 - t) * 6.0; }
        p
    };
    let r = (hue_to_rgb(p, q, h + 1.0/3.0) * 255.0) as u8;
    let g = (hue_to_rgb(p, q, h) * 255.0) as u8;
    let b = (hue_to_rgb(p, q, h - 1.0/3.0) * 255.0) as u8;
    (r, g, b)
}
