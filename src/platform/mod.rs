use std::sync::Arc;
use tiny_skia::Pixmap;
use winit::window::Window;
use softbuffer::{Context, Surface};

pub struct Platform {
    surface: Surface<Arc<Window>, Arc<Window>>,
    width:   u32,
    height:  u32,
}

impl Platform {
    pub fn new_windowed(window: Arc<Window>) -> Self {
        let context = Context::new(window.clone()).expect("Failed to create softbuffer context");
        let surface = Surface::new(&context, window.clone()).expect("Failed to create surface");
        let size = window.inner_size();
        let mut platform = Self { surface, width: size.width, height: size.height };
        platform.resize(size.width, size.height);
        platform
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width  = width.max(1);
        self.height = height.max(1);
        self.surface
            .resize(
                std::num::NonZeroU32::new(self.width).unwrap(),
                std::num::NonZeroU32::new(self.height).unwrap(),
            )
            .expect("Failed to resize surface");
    }

    pub fn render<F: FnOnce(&mut Pixmap)>(&mut self, draw: F) {
        let mut pixmap = match Pixmap::new(self.width, self.height) {
            Some(p) => p,
            None    => return,
        };

        draw(&mut pixmap);

        // Blit pixmap (RGBA) to softbuffer (ARGB / 0xAARRGGBB on little-endian)
        let mut buf = self.surface.buffer_mut().expect("Failed to get surface buffer");
        let pixels = pixmap.pixels();
        for (i, px) in pixels.iter().enumerate() {
            let r = px.red();
            let g = px.green();
            let b = px.blue();
            let a = px.alpha();
            buf[i] = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        }
        buf.present().expect("Failed to present buffer");
    }
}
