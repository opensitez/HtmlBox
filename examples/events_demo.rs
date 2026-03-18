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

struct App {
    window:   Option<Arc<Window>>,
    platform: Option<Platform>,
    renderer: Renderer,
    doc:      Option<Document>,
    width:    f32,
    mouse_pos: (f32, f32),
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = Arc::new(event_loop.create_window(Window::default_attributes().with_title("events_demo — rhtmledit").with_inner_size(winit::dpi::LogicalSize::new(1000u32, 800u32))).unwrap());
        let platform = Platform::new_windowed(window.clone());
        self.width = platform.logical_width();
        
        let doc = load_html(HTML, self.width);
        
        // ─── Interactivity ──────────────────────────────────────────────────
        
          // Card Selection — use current_target like graph_demo so selector-matched
          // element is mutated even when clicks land on child nodes.
          doc.events.add(".card", HtmlEventType::Click, Box::new(|evt| {
                let root = unsafe { &mut *(evt.root as *mut HtmlBox) };
                let target_mut = unsafe { &mut *(evt.current_target as *mut HtmlBox) };

                if dom::has_class(target_mut, "selected") {
                    dom::remove_class(target_mut, "selected");
                } else {
                    dom::add_class(target_mut, "selected");
                }
                println!("Card clicked: {:?}", target_mut.attributes.get("id"));
                // Optionally update any global UI state via root if desired.
                let _ = root; // silence unused
          }));

        // Hover Hints — use current_target so the hovered card element is reported
        doc.events.add(".card", HtmlEventType::MouseEnter, Box::new(|evt| {
            let target = unsafe { &*(evt.current_target as *const HtmlBox) };
            println!("Hovering card: {:?}", target.attributes.get("id"));
        }));

        // Keyboard navigation (global listener)
        doc.events.add("body", HtmlEventType::KeyDown, Box::new(|evt| {
            match evt.key_code {
                27 => { // ESC
                    println!("ESC pressed - clearing selection");
                }
                _ => {}
            }
        }));

        self.doc = Some(doc);
        self.window   = Some(window);
        self.platform = Some(platform);
    }

    fn window_event(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop, _window_id: winit::window::WindowId, event: WindowEvent) {
        let (window, platform) = match (self.window.as_ref(), self.platform.as_mut()) { (Some(w), Some(p)) => (w, p), _ => return };
        match event {
            WindowEvent::CloseRequested => _event_loop.exit(),
            WindowEvent::Resized(size) => {
                platform.resize(size.width, size.height);
                self.width = platform.logical_width();
                if let Some(doc) = self.doc.as_mut() {
                    LayoutEngine::new().layout(doc, self.width);
                }
                window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = (position.x as f32 / platform.scale_factor(), position.y as f32 / platform.scale_factor());
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => {
                if let Some(doc) = self.doc.as_mut() {
                    let etype = match button {
                        MouseButton::Left => HtmlEventType::Click,
                        MouseButton::Right => HtmlEventType::ContextMenu,
                        _ => HtmlEventType::Click,
                    };
                    if doc.process_mouse_event(etype, (self.mouse_pos.0, self.mouse_pos.1 + doc.scroll_y), match button { MouseButton::Left => 0, MouseButton::Middle => 1, MouseButton::Right => 2, _ => 0 }) {
                        LayoutEngine::new().layout(doc, self.width);
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(code), state: ElementState::Pressed, .. }, .. } => {
                if let Some(doc) = self.doc.as_mut() {
                    let kc = match code {
                        KeyCode::Escape => 27,
                        KeyCode::Enter => 13,
                        _ => 0,
                    };
                    if doc.process_key_event(HtmlEventType::KeyDown, kc, None, false, false, false, false) {
                        LayoutEngine::new().layout(doc, self.width);
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta { winit::event::MouseScrollDelta::LineDelta(_, y) => y * 20.0, winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32 / platform.scale_factor() };
                let mp = self.mouse_pos;
                if let Some(doc) = self.doc.as_mut() {
                    let doc_pt = (mp.0, mp.1 + doc.scroll_y);
                    doc.process_wheel_event(doc_pt, dy);
                }
                window.request_redraw();
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

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        window: None, platform: None,
        renderer: Renderer::new(),
        doc: None, width: 1000.0,
        mouse_pos: (0.0, 0.0),
    };
    event_loop.run_app(&mut app).unwrap();
}
