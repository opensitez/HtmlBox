mod platform;

use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::Window;

use rhtmledit::{load_html, Document, HtmlBox, Renderer};
use rhtmledit::{point_to_hit, offset_to_point};
use rhtmledit::dom::{EventListeners, HtmlEvent, HtmlEventType};
use rhtmledit::layout::hit_test::hit_test_box_at;
use platform::Platform;

const CARET_BLINK_MS: u64 = 500;

const DEMO_HTML: &str = r##"<!DOCTYPE html>
<html>
<head>
  <style>
    body { font-family: sans-serif; margin: 16px; color: #222; background: #fff; }
    h1   { color: #2c5aa0; font-size: 2em; margin-bottom: 8px; }
    h2   { color: #444; font-size: 1.4em; margin: 16px 0 4px; border-bottom: 2px solid #ddd; }
    p    { margin: 8px 0; line-height: 1.6; }
    .card {
      border: 1px solid #ccc; border-radius: 6px;
      padding: 12px 16px; margin: 12px 0;
      background: #f9f9f9;
    }
    .flex-row { display: flex; gap: 12px; margin: 12px 0; }
    .box {
      flex: 1; padding: 12px; border-radius: 4px;
      background: #e8f0fe; border: 1px solid #8ab4f8;
    }
    ul   { padding-left: 2em; }
    code { background: #eee; padding: 1px 4px; border-radius: 3px; font-family: monospace; }
    a    { color: #1a73e8; }
    strong { font-weight: bold; }
    em     { font-style: italic; }
    table  { border-collapse: collapse; width: 100%; margin: 12px 0; }
    th, td { border: 1px solid #ccc; padding: 6px 12px; }
    th     { background: #f0f0f0; font-weight: bold; }
    blockquote {
      border-left: 4px solid #8ab4f8; margin: 12px 0;
      padding: 8px 16px; background: #f0f5ff; color: #555;
    }
  </style>
</head>
<body>
  <h1>rhtmledit — Rust HTML/CSS Renderer</h1>
  <p contenteditable="true">A port of <strong>wxhtmledit</strong> to Rust, using <em>tiny-skia</em> for rendering
     and <em>winit</em> for windowing. Click here to edit.</p>

  <div class="card">
    <h2>Features</h2>
    <ul>
      <li>Full CSS cascade with specificity</li>
      <li>Block, inline, <strong>flexbox</strong>, and <strong>CSS grid</strong> layout</li>
      <li>Table layout with colspan/rowspan</li>
      <li>Bidirectional text (LTR/RTL)</li>
      <li>Font rendering via <code>cosmic-text</code></li>
      <li>Border, background, border-radius, shadows</li>
      <li>List markers (disc, decimal, alpha, roman)</li>
      <li>Pseudo-classes and attribute selectors</li>
      <li>HTML entities and whitespace collapsing</li>
    </ul>
  </div>

  <h2>Flexbox Demo</h2>
  <div class="flex-row">
    <div class="box"><strong>Column A</strong><br/>flex-grow: 1</div>
    <div class="box"><strong>Column B</strong><br/>flex-grow: 1</div>
    <div class="box"><strong>Column C</strong><br/>flex-grow: 1</div>
  </div>

  <h2>Typography</h2>
  <p>Normal text, <strong>bold</strong>, <em>italic</em>, <strong><em>bold italic</em></strong>,
     <code>monospace code</code>, <a href="#">link</a>.</p>

  <blockquote>
    "The goal is to have a complete, fast HTML/CSS rendering engine in pure Rust."
  </blockquote>

  <h2>Table</h2>
  <table>
    <tr><th>Property</th><th>C++ (wxhtmledit)</th><th>Rust (rhtmledit)</th></tr>
    <tr><td>Language</td><td>C++17</td><td>Rust 2021</td></tr>
    <tr><td>Rendering</td><td>wxWidgets DC</td><td>tiny-skia</td></tr>
    <tr><td>Windowing</td><td>wxWidgets</td><td>winit</td></tr>
    <tr><td>Text</td><td>wxFont</td><td>cosmic-text</td></tr>
    <tr><td>Layout</td><td>Custom engine</td><td>Ported engine</td></tr>
  </table>

  <p style="color:#888; font-size:0.85em;">rhtmledit v0.1.0 — built with tiny-skia + winit + cosmic-text</p>
</body>
</html>
"##;

// ─── Editor state ─────────────────────────────────────────────────────────────

struct EditorState {
    /// Box that owns the caret (raw pointer, valid while doc is not rebuilt).
    caret_box:    Option<*const HtmlBox>,
    /// Byte offset within caret_box's flat text.
    caret_local:  usize,
    /// Selection anchor (local offset in the same box as caret).
    sel_anchor:   usize,
    /// Inclusive selection start (local, same box as caret).
    sel_start:    usize,
    /// Exclusive selection end (local, same box as caret).
    sel_end:      usize,
    /// Caret currently visible (blink phase).
    caret_visible: bool,
    /// When the last blink phase toggle happened.
    last_blink:   Instant,
    /// Is the left mouse button down (dragging for selection)?
    mouse_down:   bool,
    /// Has the window focus?
    has_focus:    bool,
}

impl EditorState {
    fn new() -> Self {
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
        }
    }

    fn has_selection(&self) -> bool { self.sel_start < self.sel_end }

    fn caret_info(&self) -> Option<(*const HtmlBox, usize)> {
        self.caret_box.map(|p| (p, self.caret_local))
    }

    fn sel_args(&self) -> (Option<usize>, Option<usize>) {
        if self.has_selection() {
            (Some(self.sel_start), Some(self.sel_end))
        } else {
            (None, None)
        }
    }

    /// Move caret to a hit result; optionally extend selection (shift-click).
    fn set_caret_from_hit(&mut self, box_ptr: *const HtmlBox, local: usize, extend: bool) {
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

    /// Move caret left one byte (or collapse selection to left).
    fn move_left(&mut self, flat: &str, extend: bool) {
        let pos = self.caret_local;
        if !extend && self.has_selection() {
            let new = self.sel_start;
            self.collapse_to(new);
            return;
        }
        let new = prev_char_boundary(flat, pos);
        self.move_to(new, extend);
    }

    /// Move caret right one byte (or collapse selection to right).
    fn move_right(&mut self, flat: &str, extend: bool) {
        let pos = self.caret_local;
        if !extend && self.has_selection() {
            let new = self.sel_end;
            self.collapse_to(new);
            return;
        }
        let new = next_char_boundary(flat, pos);
        self.move_to(new, extend);
    }

    fn move_to(&mut self, new_pos: usize, extend: bool) {
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

    fn collapse_to(&mut self, pos: usize) {
        self.caret_local = pos;
        self.sel_anchor  = pos;
        self.sel_start   = pos;
        self.sel_end     = pos;
        self.caret_visible = true;
        self.last_blink    = Instant::now();
    }

    fn blink_update(&mut self) -> bool {
        if self.last_blink.elapsed() >= Duration::from_millis(CARET_BLINK_MS) {
            self.caret_visible = !self.caret_visible;
            self.last_blink    = Instant::now();
            true
        } else {
            false
        }
    }

    fn next_blink_deadline(&self) -> Instant {
        self.last_blink + Duration::from_millis(CARET_BLINK_MS)
    }
}

// ─── Text editing helpers ─────────────────────────────────────────────────────

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

/// Insert `ch` into the node's text at byte offset `local`, return new offset.
fn insert_char_at(node: &mut HtmlBox, local: usize, ch: char) -> usize {
    let ins = local.min(node.text.len());
    let mut buf = [0u8; 4];
    let s = ch.encode_utf8(&mut buf);
    node.text.insert_str(ins, s);
    ins + s.len()
}

/// Delete the character before `local` in the node's text. Returns new offset.
fn delete_char_before(node: &mut HtmlBox, local: usize) -> usize {
    if local == 0 || node.text.is_empty() { return 0; }
    let end = local.min(node.text.len());
    let start = prev_char_boundary(&node.text, end);
    node.text.drain(start..end);
    start
}

/// Delete the character at or after `local`. Returns same offset.
fn delete_char_at(node: &mut HtmlBox, local: usize) -> usize {
    if local >= node.text.len() { return local; }
    let start = local;
    let end   = next_char_boundary(&node.text, local);
    node.text.drain(start..end);
    start
}

/// Find the mutable box by raw pointer, depth-first.
fn find_box_mut<'a>(root: &'a mut HtmlBox, ptr: *const HtmlBox) -> Option<&'a mut HtmlBox> {
    if std::ptr::eq(root as *const HtmlBox, ptr) { return Some(root); }
    for child in &mut root.children {
        if let Some(b) = find_box_mut(child, ptr) { return Some(b); }
    }
    None
}

// ─── App ──────────────────────────────────────────────────────────────────────

struct App {
    window:    Option<Arc<Window>>,
    platform:  Option<Platform>,
    renderer:  Renderer,
    doc:       Option<Document>,
    editor:    EditorState,
    events:    EventListeners,
    scroll_y:  f32,
    /// Logical viewport width (physical / scale_factor).  Used for layout.
    width:     f32,
    /// Logical viewport height.  Used for vh/vmin/vmax resolution.
    height:    f32,
    /// HiDPI scale factor from winit.
    scale:     f32,
    mouse_x:   f32,
    mouse_y:   f32,
}

impl App {
    fn relayout(&mut self) {
        if let Some(doc) = self.doc.as_mut() {
            let mut engine = rhtmledit::LayoutEngine::new();
            engine.viewport_w = self.width;
            engine.viewport_h = self.height;
            engine.font_system = Some(&mut self.renderer.font_system as *mut _);
            engine.layout(doc, self.width);
            // Caret pointer is now stale after rebuild — reset
            self.editor.caret_box = None;
        }
    }

    fn request_redraw(&self) {
        if let Some(w) = self.window.as_ref() { w.request_redraw(); }
    }

    fn doc_pt(&self) -> (f32, f32) {
        (self.mouse_x / self.scale, (self.mouse_y / self.scale) + self.scroll_y)
    }

    /// Route a positional winit event through the HTML event listener system.
    fn dispatch_mouse_event(&mut self, etype: HtmlEventType, doc_pt: (f32, f32), button: u8) {
        if self.events.is_empty() { return; }
        if let Some(doc) = self.doc.as_ref() {
            let target = unsafe { hit_test_box_at(&doc.root, doc_pt) };
            let mut evt = HtmlEvent::new(etype);
            evt.target     = target;
            evt.client_pos = (self.mouse_x, self.mouse_y);
            evt.doc_pos    = doc_pt;
            evt.button     = button;
            self.events.dispatch(&doc.root, evt);
        }
    }

    /// Route a keyboard event through the HTML event listener system.
    fn dispatch_key_event(&mut self, etype: HtmlEventType, key_code: u32,
                          ch: Option<char>, ctrl: bool, shift: bool, alt: bool) {
        if self.events.is_empty() { return; }
        if let Some(doc) = self.doc.as_ref() {
            let mut evt = HtmlEvent::new(etype);
            evt.key_code  = key_code;
            evt.char_code = ch;
            evt.ctrl_key  = ctrl;
            evt.shift_key = shift;
            evt.alt_key   = alt;
            self.events.dispatch(&doc.root, evt);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("rhtmledit")
                    .with_inner_size(winit::dpi::LogicalSize::new(900u32, 700u32))
            ).expect("Failed to create window")
        );

        let size    = window.inner_size();
        let scale   = window.scale_factor() as f32;
        self.scale  = scale;
        self.width  = size.width  as f32 / scale;
        self.height = size.height as f32 / scale;
        self.renderer.scale = scale;
        self.doc   = Some(load_html(DEMO_HTML, self.width));

        let platform = Platform::new_windowed(window.clone());
        self.window   = Some(window);
        self.platform = Some(platform);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            // ── Window lifecycle ──────────────────────────────────────────────
            WindowEvent::CloseRequested => { event_loop.exit(); }

            WindowEvent::Focused(focused) => {
                self.editor.has_focus = focused;
                self.request_redraw();
            }

            WindowEvent::Resized(size) => {
                self.width  = size.width  as f32 / self.scale;
                self.height = size.height as f32 / self.scale;
                if let Some(p) = self.platform.as_mut() { p.resize(size.width, size.height); }
                self.relayout();
                self.request_redraw();
            }

            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = scale_factor as f32;
                self.renderer.scale = self.scale;
                if let Some(w) = self.window.as_ref() {
                    let size = w.inner_size();
                    self.width  = size.width  as f32 / self.scale;
                    self.height = size.height as f32 / self.scale;
                    if let Some(p) = self.platform.as_mut() { p.resize(size.width, size.height); }
                }
                self.relayout();
                self.request_redraw();
            }

            // ── Mouse wheel ───────────────────────────────────────────────────
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y * 20.0,
                    winit::event::MouseScrollDelta::PixelDelta(p)   => p.y as f32,
                };
                self.scroll_y = (self.scroll_y - dy).max(0.0);
                self.request_redraw();
            }

            // ── Mouse move ────────────────────────────────────────────────────
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_x = position.x as f32;
                self.mouse_y = position.y as f32;

                let pt = self.doc_pt();
                self.dispatch_mouse_event(HtmlEventType::MouseMove, pt, 0);

                if self.editor.mouse_down {
                    if let Some(doc) = self.doc.as_ref() {
                        let pt = self.doc_pt();
                        if let Some(hit) = point_to_hit(&doc.root, pt) {
                            let anchor_box = self.editor.caret_box;
                            // Only extend selection within the same box for now
                            if anchor_box == Some(hit.box_ptr) {
                                self.editor.caret_local = hit.local_offset;
                                self.editor.sel_start =
                                    self.editor.sel_anchor.min(hit.local_offset);
                                self.editor.sel_end =
                                    self.editor.sel_anchor.max(hit.local_offset);
                                self.editor.caret_visible = true;
                                self.request_redraw();
                            }
                        }
                    }
                }
            }

            // ── Mouse button ──────────────────────────────────────────────────
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                if state == ElementState::Pressed {
                    self.editor.mouse_down = true;
                    self.editor.has_focus  = true;

                    let pt = self.doc_pt();
                    self.dispatch_mouse_event(HtmlEventType::MouseDown, pt, 0);

                    if let Some(doc) = self.doc.as_ref() {
                        if let Some(hit) = point_to_hit(&doc.root, pt) {
                            self.editor.set_caret_from_hit(hit.box_ptr, hit.local_offset, false);
                            self.request_redraw();
                        }
                    }
                } else {
                    self.editor.mouse_down = false;
                    let pt = self.doc_pt();
                    self.dispatch_mouse_event(HtmlEventType::MouseUp, pt, 0);
                    self.dispatch_mouse_event(HtmlEventType::Click, pt, 0);
                }
            }

            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => {
                let pt = self.doc_pt();
                self.dispatch_mouse_event(HtmlEventType::ContextMenu, pt, 2);
            }

            // ── Keyboard ──────────────────────────────────────────────────────
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                // Dispatch KeyDown through the HTML event system first
                let key_code = winit_key_to_code(&event.logical_key);
                let ch = match &event.logical_key {
                    Key::Character(s) => s.chars().next(),
                    _ => None,
                };
                self.dispatch_key_event(HtmlEventType::KeyDown, key_code, ch, false, false, false);
                if ch.is_some() {
                    self.dispatch_key_event(HtmlEventType::KeyPress, key_code, ch, false, false, false);
                }

                let handled = self.handle_key(&event.logical_key);
                if handled { self.request_redraw(); }
            }

            // ── IME / character input ─────────────────────────────────────────
            WindowEvent::Ime(winit::event::Ime::Commit(s)) => {
                for ch in s.chars() { self.insert_char(ch); }
                self.request_redraw();
            }

            // ── Redraw ────────────────────────────────────────────────────────
            WindowEvent::RedrawRequested => {
                let doc = match self.doc.as_ref() { Some(d) => d, None => return };
                let (sel_start, sel_end) = self.editor.sel_args();
                let caret_info           = self.editor.caret_info();
                let caret_visible        = self.editor.caret_visible && self.editor.has_focus;
                let has_focus            = self.editor.has_focus;
                let scroll_y             = self.scroll_y;
                self.renderer.scale      = self.scale;
                let renderer             = &mut self.renderer;

                if let Some(platform) = self.platform.as_mut() {
                    platform.render(|pixmap| {
                        renderer.render(
                            doc, pixmap,
                            0.0, scroll_y,
                            sel_start, sel_end,
                            caret_info, caret_visible, has_focus,
                        );
                    });
                }

                // Schedule next blink
                if let Some(w) = self.window.as_ref() {
                    let _ = w; // keep alive
                }
                event_loop.set_control_flow(
                    ControlFlow::WaitUntil(self.editor.next_blink_deadline())
                );
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        // Blink tick
        if self.editor.has_focus && self.editor.blink_update() {
            self.request_redraw();
        }
    }
}

impl App {
    /// Returns true if the event was consumed (needs redraw).
    fn handle_key(&mut self, key: &Key) -> bool {
        match key {
            Key::Named(NamedKey::ArrowLeft) => {
                if let Some(doc) = self.doc.as_ref() {
                    if let Some(box_ptr) = self.editor.caret_box {
                        let flat = rhtmledit::layout::inline_layout::collect_flat_text(
                            unsafe { &*box_ptr }
                        );
                        self.editor.move_left(&flat, false);
                        return true;
                    }
                }
                false
            }
            Key::Named(NamedKey::ArrowRight) => {
                if let Some(_doc) = self.doc.as_ref() {
                    if let Some(box_ptr) = self.editor.caret_box {
                        let flat = rhtmledit::layout::inline_layout::collect_flat_text(
                            unsafe { &*box_ptr }
                        );
                        self.editor.move_right(&flat, false);
                        return true;
                    }
                }
                false
            }
            Key::Named(NamedKey::Home) => {
                self.editor.move_to(0, false);
                true
            }
            Key::Named(NamedKey::End) => {
                if let Some(box_ptr) = self.editor.caret_box {
                    let flat = rhtmledit::layout::inline_layout::collect_flat_text(
                        unsafe { &*box_ptr }
                    );
                    self.editor.move_to(flat.len(), false);
                }
                true
            }
            Key::Named(NamedKey::Backspace) => {
                self.delete_before_caret();
                true
            }
            Key::Named(NamedKey::Delete) => {
                self.delete_at_caret();
                true
            }
            Key::Named(NamedKey::Enter) => {
                self.insert_char('\n');
                true
            }
            Key::Character(s) => {
                for ch in s.chars() {
                    if !ch.is_control() { self.insert_char(ch); }
                }
                !s.is_empty()
            }
            _ => false,
        }
    }

    fn insert_char(&mut self, ch: char) {
        let box_ptr = match self.editor.caret_box { Some(p) => p, None => return };
        if let Some(doc) = self.doc.as_mut() {
            if let Some(node) = find_box_mut(&mut doc.root, box_ptr) {
                // Delete selection first
                if self.editor.has_selection() {
                    let s = self.editor.sel_start;
                    let e = self.editor.sel_end.min(node.text.len());
                    if s < e { node.text.drain(s..e); }
                    self.editor.collapse_to(s);
                }
                let new_off = insert_char_at(node, self.editor.caret_local, ch);
                self.editor.collapse_to(new_off);
                // Re-layout so line_cache is updated
                let mut engine = rhtmledit::LayoutEngine::new();
                engine.font_system = Some(&mut self.renderer.font_system as *mut _);
                engine.layout(doc, self.width);
                // Pointer is still valid (tree structure unchanged, only text modified)
            }
        }
    }

    fn delete_before_caret(&mut self) {
        let box_ptr = match self.editor.caret_box { Some(p) => p, None => return };
        if let Some(doc) = self.doc.as_mut() {
            if let Some(node) = find_box_mut(&mut doc.root, box_ptr) {
                if self.editor.has_selection() {
                    let s = self.editor.sel_start;
                    let e = self.editor.sel_end.min(node.text.len());
                    if s < e { node.text.drain(s..e); }
                    self.editor.collapse_to(s);
                } else {
                    let new_off = delete_char_before(node, self.editor.caret_local);
                    self.editor.collapse_to(new_off);
                }
                let mut engine = rhtmledit::LayoutEngine::new();
                engine.font_system = Some(&mut self.renderer.font_system as *mut _);
                engine.layout(doc, self.width);
            }
        }
    }

    fn delete_at_caret(&mut self) {
        let box_ptr = match self.editor.caret_box { Some(p) => p, None => return };
        if let Some(doc) = self.doc.as_mut() {
            if let Some(node) = find_box_mut(&mut doc.root, box_ptr) {
                if self.editor.has_selection() {
                    let s = self.editor.sel_start;
                    let e = self.editor.sel_end.min(node.text.len());
                    if s < e { node.text.drain(s..e); }
                    self.editor.collapse_to(s);
                } else {
                    let new_off = delete_char_at(node, self.editor.caret_local);
                    self.editor.collapse_to(new_off);
                }
                let mut engine = rhtmledit::LayoutEngine::new();
                engine.font_system = Some(&mut self.renderer.font_system as *mut _);
                engine.layout(doc, self.width);
            }
        }
    }
}

/// Map a winit `Key` to a numeric key code (matches common browser key codes).
fn winit_key_to_code(key: &Key) -> u32 {
    match key {
        Key::Named(NamedKey::Enter)     => 13,
        Key::Named(NamedKey::Backspace) => 8,
        Key::Named(NamedKey::Delete)    => 46,
        Key::Named(NamedKey::Escape)    => 27,
        Key::Named(NamedKey::Tab)       => 9,
        Key::Named(NamedKey::ArrowLeft) => 37,
        Key::Named(NamedKey::ArrowUp)   => 38,
        Key::Named(NamedKey::ArrowRight)=> 39,
        Key::Named(NamedKey::ArrowDown) => 40,
        Key::Named(NamedKey::Home)      => 36,
        Key::Named(NamedKey::End)       => 35,
        Key::Named(NamedKey::PageUp)    => 33,
        Key::Named(NamedKey::PageDown)  => 34,
        Key::Character(s) => s.chars().next().map(|c| c as u32).unwrap_or(0),
        _ => 0,
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App {
        window:   None,
        platform: None,
        renderer: Renderer::new(),
        doc:      None,
        editor:   EditorState::new(),
        events:   EventListeners::new(),
        scroll_y: 0.0,
        width:    900.0,
        height:   700.0,
        scale:    1.0,
        mouse_x:  0.0,
        mouse_y:  0.0,
    };

    // ── Example event bindings ────────────────────────────────────────────────
    // Register HTML event listeners exactly like in the C++ events_demo:
    //
    //   app.events.add(".card", HtmlEventType::Click, Box::new(|evt| {
    //       println!("card clicked at {:?}", evt.doc_pos);
    //   }));
    //
    //   app.events.add("a", HtmlEventType::Click, Box::new(|evt| {
    //       evt.prevent_default();
    //       println!("link clicked");
    //   }));
    //
    //   app.events.add("*", HtmlEventType::KeyDown, Box::new(|evt| {
    //       println!("key {:?}", evt.key_code);
    //   }));
    // ─────────────────────────────────────────────────────────────────────────

    event_loop.run_app(&mut app).expect("Failed to run app");
}
