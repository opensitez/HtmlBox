//! The spinner half of `<input type=number>` — HTML §4.10.5.1.13.
//!
//! Lifted from `vybe_widgets::numeric`, which draws the same control with the
//! same tiny-skia calls. What did NOT come across is the toolkit's shape: that
//! one is a `PanelWidget` with a name, an id, a rect, an event queue and its own
//! value/min/max/step state. Here the DOM holds the value and the cascade holds
//! the box, so this is only the chrome CSS cannot express — two arrows in a
//! well, drawn at the element's right-hand edge.
//!
//! The field itself is not here: an `<input type=number>` IS a text field with a
//! stepper on it, and the field is a CSS box with a text run, which the engine
//! already draws.

use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// The stepper well and its two arrows.
pub struct Stepper {
    pub width: f32,
    pub height: f32,
    /// Whether the control is disabled — the arrows grey out, which is the only
    /// state a still frame can show. `:disabled` handles the rest in CSS.
    pub disabled: bool,
}

impl Stepper {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            disabled: false,
        }
    }

    /// How wide the stepper well is for a control of this height.
    ///
    /// Proportional to the height rather than fixed, so a control in a large
    /// font gets arrows to match instead of two specks in a corner. Clamped
    /// because a very tall `<input>` should not hand half its width to chrome.
    pub fn well_width(height: f32) -> f32 {
        (height * 0.55).clamp(12.0, 22.0)
    }

    /// Paint at the RIGHT-HAND edge of a control whose box is `x, y, w, h`.
    ///
    /// Takes the control's rect rather than its own position because that is
    /// what the caller has, and because the well is defined relative to the
    /// element's trailing edge.
    pub fn paint(&self, pixmap: &mut Pixmap, x: f32, y: f32, scale: f32) {
        if self.width <= 0.0 || self.height <= 4.0 {
            return;
        }
        let ts = Transform::from_scale(scale, scale);
        let mut paint = Paint::default();
        paint.anti_alias = true;
        let bw = Self::well_width(self.height);
        let btn_x = x + self.width - bw;

        // The well, and the two dividers that make it read as two buttons.
        paint.set_color_rgba8(240, 240, 240, 255);
        if let Some(rect) = tiny_skia::Rect::from_xywh(btn_x, y + 1.0, bw - 1.0, self.height - 2.0)
        {
            pixmap.fill_rect(rect, &paint, ts, None);
        }
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        paint.set_color_rgba8(160, 160, 160, 255);
        let mid_y = y + self.height / 2.0;
        let mut pb = PathBuilder::new();
        pb.move_to(btn_x, y + 1.0);
        pb.line_to(btn_x, y + self.height - 1.0);
        pb.move_to(btn_x, mid_y);
        pb.line_to(x + self.width - 1.0, mid_y);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, ts, None);
        }

        // The arrows. Sized from the well so they scale with the control.
        let arrow = (bw * 0.22).clamp(2.5, 5.0);
        let center_x = btn_x + bw / 2.0;
        if self.disabled {
            paint.set_color_rgba8(160, 160, 160, 255);
        } else {
            paint.set_color_rgba8(60, 60, 60, 255);
        }
        for (center_y, up) in [
            (y + self.height * 0.25, true),
            (y + self.height * 0.75, false),
        ] {
            let tip = if up { -arrow } else { arrow };
            let base = if up { arrow * 0.6 } else { -arrow * 0.6 };
            let mut pb = PathBuilder::new();
            pb.move_to(center_x, center_y + tip);
            pb.line_to(center_x + arrow, center_y + base);
            pb.line_to(center_x - arrow, center_y + base);
            pb.close();
            if let Some(path) = pb.finish() {
                pixmap.fill_path(&path, &paint, FillRule::Winding, ts, None);
            }
        }
    }
}
