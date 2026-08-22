//! Range slider widget — standalone tiny-skia rendered slider.

use tiny_skia::*;
use super::{WidgetColors, rounded_rect_path, circle_path};

pub struct Slider {
    pub value: f32,     // 0.0..1.0
    pub min: f32,
    pub max: f32,
    pub disabled: bool,
    pub focused: bool,
    pub dragging: bool,
    pub colors: WidgetColors,
    pub width: f32,
    pub height: f32,
    pub track_height: f32,
    pub thumb_radius: f32,
}

impl Slider {
    pub fn new(min: f32, max: f32, value: f32) -> Self {
        let pct = if max > min { (value - min) / (max - min) } else { 0.0 };
        Self {
            value: pct.clamp(0.0, 1.0),
            min,
            max,
            disabled: false,
            focused: false,
            dragging: false,
            colors: WidgetColors::default(),
            width: 200.0,
            height: 20.0,
            track_height: 4.0,
            thumb_radius: 8.0,
        }
    }

    /// Get the actual value (mapped from 0..1 to min..max).
    pub fn actual_value(&self) -> f32 {
        self.min + self.value * (self.max - self.min)
    }

    pub fn paint(&self, pixmap: &mut Pixmap, x: f32, y: f32, scale: f32) {
        let ts = Transform::from_scale(scale, scale);
        let mut paint = Paint::default();
        paint.anti_alias = true;

        let track_y = y + (self.height - self.track_height) / 2.0;
        let thumb_x = x + self.thumb_radius + self.value * (self.width - self.thumb_radius * 2.0);
        let thumb_y = y + self.height / 2.0;

        // Track background
        paint.set_color_rgba8(200, 200, 200, 255);
        if let Some(path) = rounded_rect_path(x, track_y, self.width, self.track_height, 2.0) {
            pixmap.fill_path(&path, &paint, FillRule::Winding, ts, None);
        }

        // Filled portion
        let (r, g, b, a) = self.colors.accent;
        paint.set_color_rgba8(r, g, b, a);
        let fill_w = thumb_x - x;
        if fill_w > 0.0 {
            if let Some(path) = rounded_rect_path(x, track_y, fill_w, self.track_height, 2.0) {
                pixmap.fill_path(&path, &paint, FillRule::Winding, ts, None);
            }
        }

        // Thumb
        let (r, g, b, a) = self.colors.background;
        paint.set_color_rgba8(r, g, b, a);
        if let Some(path) = circle_path(thumb_x, thumb_y, self.thumb_radius) {
            pixmap.fill_path(&path, &paint, FillRule::Winding, ts, None);
        }
        let (r, g, b, a) = if self.focused { self.colors.focus_ring } else { self.colors.border };
        paint.set_color_rgba8(r, g, b, a);
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        if let Some(path) = circle_path(thumb_x, thumb_y, self.thumb_radius) {
            pixmap.stroke_path(&path, &paint, &stroke, ts, None);
        }
    }

    pub fn measure(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    /// Handle mouse down — start dragging if on thumb.
    pub fn mouse_down(&mut self, x: f32, _y: f32) -> bool {
        if self.disabled { return false; }
        self.dragging = true;
        self.set_from_x(x);
        true
    }

    /// Handle mouse move during drag.
    pub fn mouse_move(&mut self, x: f32) {
        if self.dragging {
            self.set_from_x(x);
        }
    }

    /// Handle mouse up — stop dragging.
    pub fn mouse_up(&mut self) {
        self.dragging = false;
    }

    fn set_from_x(&mut self, x: f32) {
        let usable = self.width - self.thumb_radius * 2.0;
        let pct = (x - self.thumb_radius) / usable;
        self.value = pct.clamp(0.0, 1.0);
    }
}
