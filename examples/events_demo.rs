use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{WindowEvent, ElementState, MouseButton, KeyEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;
use winit::keyboard::{PhysicalKey, KeyCode};

use rhtmledit::{load_html, Document, Renderer, LayoutEngine, HtmlBox};
use rhtmledit::platform::Platform;
use rhtmledit::dom::{self, HtmlEventType};

const HTML: &str = include_str!("html/events.html");

const COLS: &[(&str, &str)] = &[
    ("col-backlog",  "body-backlog"),
    ("col-todo",     "body-todo"),
    ("col-progress", "body-progress"),
    ("col-review",   "body-review"),
    ("col-done",     "body-done"),
];

struct DragState {
    /// Card id being dragged, empty when idle.
    source_id:   String,
    /// Title text of card being dragged.
    source_title: String,
    /// Mouse position when drag was initiated.
    start_pos:   (f32, f32),
    /// Whether the drag threshold has been crossed.
    active:      bool,
    /// Column body id we are currently hovering over, if any.
    target_body: Option<String>,
}

impl DragState {
    fn idle() -> Self { Self { source_id: String::new(), source_title: String::new(), start_pos: (0.0, 0.0), active: false, target_body: None } }
    fn has_source(&self) -> bool { !self.source_id.is_empty() }
}

struct App {
    window:   Option<Arc<Window>>,
    platform: Option<Platform>,
    renderer: Renderer,
    doc:      Option<Document>,
    width:    f32,
    mouse_pos: (f32, f32),
    drag:     DragState,
    mouse_down: bool,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = Arc::new(event_loop.create_window(
            Window::default_attributes()
                .with_title("events_demo — rhtmledit")
                .with_inner_size(winit::dpi::LogicalSize::new(1000u32, 800u32))
        ).unwrap());
        let platform = Platform::new_windowed(window.clone());
        self.width = platform.logical_width();

        let doc = load_html(HTML, self.width);

        // Card selection on click (only fires when not dragging — handled below)
        doc.events.add(".card", HtmlEventType::Click, Box::new(|evt| {
            let root = unsafe { &mut *(evt.root as *mut HtmlBox) };
            let target_mut = unsafe { &mut *(evt.current_target as *mut HtmlBox) };
            // Deselect all cards first
            deselect_all(root);
            // Select this card
            dom::add_class(target_mut, "card-selected");
            if let Some(id) = target_mut.attributes.get("id") {
                let title = get_text_of_class(target_mut, "card-title");
                if let Some(info) = dom::query_selector_mut(root, "#selected-info") {
                    dom::set_text_content(info, &format!("{} ({})", title, id));
                }
            }
            let _ = root;
        }));

        self.doc      = Some(doc);
        self.window   = Some(window);
        self.platform = Some(platform);
    }

    fn window_event(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, _window_id: winit::window::WindowId, event: WindowEvent) {
        let (window, platform) = match (self.window.as_ref(), self.platform.as_mut()) {
            (Some(w), Some(p)) => (w, p),
            _ => return,
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                platform.resize(size.width, size.height);
                self.width = platform.logical_width();
                if let Some(doc) = self.doc.as_mut() {
                    LayoutEngine::new().layout(doc, self.width);
                }
                window.request_redraw();
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y * 20.0,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32 / platform.scale_factor(),
                };
                if let Some(doc) = self.doc.as_mut() {
                    let mp = self.mouse_pos;
                    doc.process_wheel_event((mp.0, mp.1 + doc.scroll_y), dy);
                }
                window.request_redraw();
            }

            WindowEvent::CursorMoved { position, .. } => {
                let sf = platform.scale_factor();
                let mx = position.x as f32 / sf;
                let my = position.y as f32 / sf;
                self.mouse_pos = (mx, my);

                if self.mouse_down && self.drag.has_source() {
                    let dx = mx - self.drag.start_pos.0;
                    let dy = my - self.drag.start_pos.1;

                    if !self.drag.active && (dx * dx + dy * dy).sqrt() > 5.0 {
                        // Cross drag threshold — start the drag
                        self.drag.active = true;
                        if let Some(doc) = self.doc.as_mut() {
                            drag_start(doc, &self.drag.source_id, &self.drag.source_title);
                            doc.recascade();
                            LayoutEngine::new().layout(doc, self.width);
                        }
                    }

                    if self.drag.active {
                        if let Some(doc) = self.doc.as_mut() {
                            let scroll_y = doc.scroll_y;
                            let doc_y = my + scroll_y;

                            // Find which column body we're hovering over
                            let new_target = find_target_body(&doc.root, mx, doc_y);

                            if new_target != self.drag.target_body {
                                // Update column highlights
                                update_drop_highlights(doc, &new_target);
                                self.drag.target_body = new_target;
                                doc.recascade();
                                LayoutEngine::new().layout(doc, self.width);
                            }

                            // Update ghost position (always, even without recascade)
                            if let Some(ghost) = dom::query_selector_mut(&mut doc.root, "#drag-ghost") {
                                dom::set_style_property(ghost, "left", &format!("{}px", mx + 12.0));
                                dom::set_style_property(ghost, "top",  &format!("{}px", doc_y + 8.0));
                            }
                            // Re-layout only for ghost position
                            LayoutEngine::new().layout(doc, self.width);
                        }
                        window.request_redraw();
                    }
                } else if !self.mouse_down {
                    // Normal hover — let process_mouse_event handle MouseEnter/Leave
                    if let Some(doc) = self.doc.as_mut() {
                        let doc_pt = (mx, my + doc.scroll_y);
                        if doc.process_mouse_event(HtmlEventType::MouseMove, doc_pt, 0) {
                            window.request_redraw();
                        }
                    }
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let (mx, my) = self.mouse_pos;
                match (state, button) {
                    (ElementState::Pressed, MouseButton::Left) => {
                        self.mouse_down = true;
                        if let Some(doc) = self.doc.as_mut() {
                            let doc_pt = (mx, my + doc.scroll_y);
                            // Check if we pressed on a card
                            let card_id = hit_card_id(&doc.root, doc_pt);
                            if let Some(id) = card_id {
                                let title = {
                                    let card = dom::query_selector(&doc.root, &format!("#{}", id));
                                    card.map(|c| get_text_of_class(c, "card-title")).unwrap_or_default()
                                };
                                self.drag = DragState {
                                    source_id: id,
                                    source_title: title,
                                    start_pos: (mx, my),
                                    active: false,
                                    target_body: None,
                                };
                            } else {
                                // Clicked outside a card — clear selection
                                if doc.process_mouse_event(HtmlEventType::MouseDown, doc_pt, 0) {
                                    LayoutEngine::new().layout(doc, self.width);
                                    window.request_redraw();
                                }
                            }
                        }
                    }

                    (ElementState::Released, MouseButton::Left) => {
                        self.mouse_down = false;
                        if self.drag.active {
                            // Complete the drop
                            if let Some(doc) = self.doc.as_mut() {
                                let dropped = if let Some(ref body_id) = self.drag.target_body.clone() {
                                    drop_card(doc, &self.drag.source_id, body_id)
                                } else {
                                    false
                                };
                                drag_end(doc, &self.drag.source_id, dropped);
                                doc.recascade();
                                LayoutEngine::new().layout(doc, self.width);
                                window.request_redraw();
                            }
                            self.drag = DragState::idle();
                        } else if self.drag.has_source() {
                            // Short press = click — fire click event on the card
                            self.drag = DragState::idle();
                            if let Some(doc) = self.doc.as_mut() {
                                let doc_pt = (mx, my + doc.scroll_y);
                                if doc.process_mouse_event(HtmlEventType::Click, doc_pt, 0) {
                                    LayoutEngine::new().layout(doc, self.width);
                                    window.request_redraw();
                                }
                            }
                        }
                    }

                    (ElementState::Pressed, MouseButton::Right) => {
                        if let Some(doc) = self.doc.as_mut() {
                            let doc_pt = (mx, my + doc.scroll_y);
                            if doc.process_mouse_event(HtmlEventType::ContextMenu, doc_pt, 2) {
                                LayoutEngine::new().layout(doc, self.width);
                                window.request_redraw();
                            }
                        }
                    }

                    _ => {}
                }
            }

            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(code), state: ElementState::Pressed, .. }, .. } => {
                if let Some(doc) = self.doc.as_mut() {
                    let kc = match code {
                        KeyCode::Escape => 27,
                        KeyCode::Enter  => 13,
                        KeyCode::Tab    => 9,
                        KeyCode::Delete => 46,
                        _ => 0,
                    };
                    if kc != 0 && doc.process_key_event(HtmlEventType::KeyDown, kc, None, false, false, false, false) {
                        LayoutEngine::new().layout(doc, self.width);
                        window.request_redraw();
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                if let Some(doc) = self.doc.as_mut() {
                    let renderer = &mut self.renderer;
                    platform.render(|scale, pixmap| { renderer.render(doc, pixmap, scale); });
                }
            }

            _ => {}
        }
    }
}

// ── Drag helpers ──────────────────────────────────────────────────────────────

/// Find the id of the top-most card under `doc_pt`, if any.
fn hit_card_id(root: &HtmlBox, doc_pt: (f32, f32)) -> Option<String> {
    use rhtmledit::layout::hit_test::point_to_hit;
    let hit = point_to_hit(root, doc_pt, 0)?;
    // Walk from hit box up through ancestors to find one with class "card"
    find_card_ancestor(root, hit.box_ptr)
}

/// Recursively search for the HtmlBox matching `ptr` and return its card ancestor id.
fn find_card_ancestor(node: &HtmlBox, target: *const HtmlBox) -> Option<String> {
    // Check if this node matches the target
    if std::ptr::eq(node as *const HtmlBox, target) {
        if dom::has_class(node, "card") {
            return node.attributes.get("id").cloned();
        }
    }
    for child in &node.children {
        if let Some(id) = find_card_in_subtree(child, target) {
            return Some(id);
        }
    }
    None
}

/// Returns the card id if `target` is found within `node`'s subtree.
/// Propagates the nearest ancestor `.card` id upward.
fn find_card_in_subtree(node: &HtmlBox, target: *const HtmlBox) -> Option<String> {
    let is_target = std::ptr::eq(node as *const HtmlBox, target);
    // Check children first
    let child_result = node.children.iter().find_map(|c| find_card_in_subtree(c, target));

    if is_target || child_result.is_some() {
        // This node is on the path to the target — if it's a card, return its id
        if dom::has_class(node, "card") {
            return node.attributes.get("id").cloned();
        }
        // Otherwise propagate upward
        return child_result;
    }
    None
}

/// Find which column body id contains the point `(mx, doc_y)`.
fn find_target_body(root: &HtmlBox, mx: f32, doc_y: f32) -> Option<String> {
    for &(_, body_id) in COLS {
        if let Some(body) = dom::query_selector(root, &format!("#{}", body_id)) {
            let r = body.border_rect;
            // Use column x-range but full vertical extent of body
            if mx >= r.x && mx < r.x + r.w && doc_y >= r.y && doc_y < r.y + r.h {
                return Some(body_id.to_string());
            }
        }
    }
    // Fallback: match by column header area too (use col-* instead of body-*)
    for &(col_id, body_id) in COLS {
        if let Some(col) = dom::query_selector(root, &format!("#{}", col_id)) {
            let r = col.border_rect;
            if mx >= r.x && mx < r.x + r.w && doc_y >= r.y && doc_y < r.y + r.h {
                return Some(body_id.to_string());
            }
        }
    }
    None
}

/// Called when drag threshold is crossed. Shows ghost and banner, marks card as dragging.
fn drag_start(doc: &mut Document, card_id: &str, card_title: &str) {
    let root = &mut doc.root;

    // Mark source card as dragging
    if let Some(card) = dom::query_selector_mut(root, &format!("#{}", card_id)) {
        dom::add_class(card, "card-dragging");
    }

    // Show drag banner
    if let Some(banner) = dom::query_selector_mut(root, "#drag-banner") {
        dom::add_class(banner, "drag-banner-visible");
    }
    if let Some(el) = dom::query_selector_mut(root, "#drag-title") {
        dom::set_text_content(el, card_title);
    }

    // Show and configure drag ghost
    if let Some(ghost) = dom::query_selector_mut(root, "#drag-ghost") {
        dom::add_class(ghost, "drag-ghost-visible");
    }
    if let Some(el) = dom::query_selector_mut(root, "#ghost-title") {
        dom::set_text_content(el, card_title);
    }

    // Show all drop placeholders
    for &(_, body_id) in COLS {
        let sel = format!("#ph-{}", &body_id["body-".len()..]);
        if let Some(ph) = dom::query_selector_mut(root, &sel) {
            dom::set_style_property(ph, "display", "block");
        }
    }
}

/// Update which column is highlighted as the current drop target.
fn update_drop_highlights(doc: &mut Document, new_target: &Option<String>) {
    let root = &mut doc.root;
    for &(col_id, _) in COLS {
        if let Some(col) = dom::query_selector_mut(root, &format!("#{}", col_id)) {
            dom::remove_class(col, "column-drop-active");
        }
    }
    if let Some(ref body_id) = new_target {
        // Derive col id from body id: "body-backlog" → "col-backlog"
        let col_id = format!("col-{}", &body_id["body-".len()..]);
        if let Some(col) = dom::query_selector_mut(root, &format!("#{}", col_id)) {
            dom::add_class(col, "column-drop-active");
        }
        if let Some(el) = dom::query_selector_mut(root, "#drag-target-col") {
            let col_name = col_id.replacen("col-", "→ ", 1);
            dom::set_text_content(el, &col_name);
        }
    } else {
        if let Some(el) = dom::query_selector_mut(root, "#drag-target-col") {
            dom::set_text_content(el, "");
        }
    }
}

/// Move the card to the target column body. Returns true on success.
fn drop_card(doc: &mut Document, card_id: &str, target_body_id: &str) -> bool {
    let root = &mut doc.root;

    // Find which body currently contains the card
    let src_body_id = {
        let mut found: Option<String> = None;
        for &(_, body_id) in COLS {
            if let Some(body) = dom::query_selector(root, &format!("#{}", body_id)) {
                if body.children.iter().any(|c| c.attributes.get("id").map(|s| s.as_str()) == Some(card_id)) {
                    found = Some(body_id.to_string());
                    break;
                }
            }
        }
        match found { Some(id) => id, None => return false }
    };

    // Don't drop onto the same column
    if src_body_id == target_body_id {
        return false;
    }

    // Remove card from source body
    let card = {
        let src_body = match dom::query_selector_mut(root, &format!("#{}", src_body_id)) {
            Some(b) => b, None => return false,
        };
        let card_ptr = match src_body.children.iter()
            .find(|c| c.attributes.get("id").map(|s| s.as_str()) == Some(card_id))
            .map(|c| c as *const HtmlBox)
        {
            Some(p) => p, None => return false,
        };
        match dom::remove_child(src_body, card_ptr) {
            Some(c) => c, None => return false,
        }
    };

    // Append to target body after the placeholder (first child)
    match dom::query_selector_mut(root, &format!("#{}", target_body_id)) {
        Some(target_body) => {
            let placeholder_ptr = target_body.children.first().map(|c| c as *const HtmlBox);
            if let Some(ph_ptr) = placeholder_ptr {
                dom::insert_after(target_body, ph_ptr, card);
            } else {
                dom::append_child(target_body, card);
            }
        }
        None => return false,
    }

    // Remove card-dragging class (card is now in new location)
    if let Some(card) = dom::query_selector_mut(root, &format!("#{}", card_id)) {
        dom::remove_class(card, "card-dragging");
    }

    true
}

/// Clean up after drag ends (success or cancelled).
fn drag_end(doc: &mut Document, card_id: &str, _dropped: bool) {
    let root = &mut doc.root;

    // Remove dragging style from card (in case drop was cancelled)
    if let Some(card) = dom::query_selector_mut(root, &format!("#{}", card_id)) {
        dom::remove_class(card, "card-dragging");
    }

    // Hide ghost and banner
    if let Some(ghost) = dom::query_selector_mut(root, "#drag-ghost") {
        dom::remove_class(ghost, "drag-ghost-visible");
    }
    if let Some(banner) = dom::query_selector_mut(root, "#drag-banner") {
        dom::remove_class(banner, "drag-banner-visible");
    }

    // Remove all column highlights
    for &(col_id, _) in COLS {
        if let Some(col) = dom::query_selector_mut(root, &format!("#{}", col_id)) {
            dom::remove_class(col, "column-drop-active");
        }
    }

    // Hide all placeholders
    for &(_, body_id) in COLS {
        let ph_suffix = &body_id["body-".len()..];
        if let Some(ph) = dom::query_selector_mut(root, &format!("#ph-{}", ph_suffix)) {
            dom::set_style_property(ph, "display", "none");
        }
    }
}

/// Deselect all cards.
fn deselect_all(root: &mut HtmlBox) {
    fn walk(node: &mut HtmlBox) {
        if dom::has_class(node, "card") { dom::remove_class(node, "card-selected"); }
        for child in node.children.iter_mut() { walk(child); }
    }
    walk(root);
}

/// Get the text content of the first descendant with `class_name`.
fn get_text_of_class<'a>(node: &'a HtmlBox, class_name: &str) -> String {
    fn walk<'a>(node: &'a HtmlBox, class_name: &str) -> Option<&'a HtmlBox> {
        if dom::has_class(node, class_name) { return Some(node); }
        for child in &node.children { if let Some(b) = walk(child, class_name) { return Some(b); } }
        None
    }
    walk(node, class_name).map(|b| dom::get_text_content(b)).unwrap_or_default()
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        window: None, platform: None,
        renderer: Renderer::new(),
        doc: None, width: 1000.0,
        mouse_pos: (0.0, 0.0),
        drag: DragState::idle(),
        mouse_down: false,
    };
    event_loop.run_app(&mut app).unwrap();
}
