/// Port of wxhtmledit/examples/demo.cpp
/// Full HTML/CSS feature showcase.

use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{WindowEvent, MouseButton, ElementState};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

use rhtmledit::{load_html, Document, Renderer, LayoutEngine, HtmlEventType};
use rhtmledit::platform::Platform;

const HTML: &str = include_str!("html/demo.html");

struct App {
    window:     Option<Arc<Window>>,
    platform:   Option<Platform>,
    renderer:   Renderer,
    doc:        Option<Document>,
    width:      f32,
    mouse_pos:  (f32, f32),
}

impl App {
    fn request_redraw(&self) { if let Some(w) = self.window.as_ref() { w.request_redraw(); } }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("demo — rhtmledit")
                    .with_inner_size(winit::dpi::LogicalSize::new(900u32, 700u32))
            ).unwrap()
        );
        let platform = Platform::new_windowed(window.clone());
        self.width = platform.logical_width();
        let doc = load_html(HTML, self.width);
        self.doc = Some(doc);
        self.window   = Some(window);
        self.platform = Some(platform);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
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
                self.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let sf = platform.scale_factor();
                self.mouse_pos = (position.x as f32 / sf, position.y as f32 / sf);
                let mp = self.mouse_pos;
                if let Some(doc) = self.doc.as_mut() {
                    let pt = (mp.0, mp.1 + doc.scroll_y);
                    if doc.process_mouse_event(HtmlEventType::MouseMove, pt, 0) {
                        self.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let etype = if state == ElementState::Pressed { HtmlEventType::MouseDown } else { HtmlEventType::MouseUp };
                let bt = match button {
                    MouseButton::Left   => 0,
                    MouseButton::Middle => 1,
                    MouseButton::Right  => 2,
                    _ => 0,
                };
                let mp = self.mouse_pos;
                if let Some(doc) = self.doc.as_mut() {
                    let pt = (mp.0, mp.1 + doc.scroll_y);
                    if doc.process_mouse_event(etype, pt, bt) {
                        self.request_redraw();
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y * 20.0,
                    winit::event::MouseScrollDelta::PixelDelta(p)  => p.y as f32 / platform.scale_factor(),
                };
                let mp = self.mouse_pos;
                if let Some(doc) = self.doc.as_mut() {
                    let doc_pt = (mp.0, mp.1 + doc.scroll_y);
                    doc.process_wheel_event(doc_pt, dy);
                }
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let doc = match self.doc.as_mut() { Some(d) => d, None => return };
                let renderer = &mut self.renderer;
                platform.render(|scale, pixmap| {
                    renderer.render(doc, pixmap, scale);
                });
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
        doc: None, width: 900.0,
        mouse_pos: (0.0, 0.0),
    };
    event_loop.run_app(&mut app).unwrap();
}
