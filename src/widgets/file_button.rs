//! `<input type=file>` — HTML §4.10.5.1.18.
//!
//! Lifted from `vybe_widgets::file_input`, keeping the drawing and dropping the
//! toolkit's `PanelWidget` shell, its pressed/disabled state machine and the
//! chooser it opens. The CHOOSER is platform chrome — a modal the operating
//! system owns — and belongs no more in a layout engine than the colour
//! picker's popup does.
//!
//! What the page lays out is a button and a label beside it, and that is what
//! this draws. The two labels themselves are drawn by the replay, which is
//! where the font context lives — the same division `select` already uses, with
//! the widget painting the chrome and the caller painting the text.

use tiny_skia::{FillRule, Paint, Pixmap, Stroke, Transform};

use super::rounded_rect_path;

/// The button half of a file input.
pub struct FileButton {
    pub width: f32,
    pub height: f32,
    pub disabled: bool,
}

/// What a file input shows when nothing is chosen.
///
/// ⛔ This is the LABEL, not the value: `input.value` for an empty file control
/// is the empty string, and a control that reported this text as its value
/// would submit it. The toolkit's own tests make the same point.
pub const NOTHING_CHOSEN: &str = "No file chosen";

/// The button's label. UA-defined text, which every browser spells this way.
pub const CHOOSE: &str = "Choose File";

impl FileButton {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            disabled: false,
        }
    }

    /// How wide the button is, given the label's measured width.
    ///
    /// Measured rather than fixed: "Choose File" in a 24px font does not fit a
    /// 90px button, and a control whose chrome clips its own label is the kind
    /// of thing that only shows up on someone else's machine.
    pub fn width_for(label_width: f32) -> f32 {
        label_width + 16.0
    }

    pub fn paint(&self, pixmap: &mut Pixmap, x: f32, y: f32, scale: f32) {
        if self.width <= 0.0 || self.height <= 0.0 {
            return;
        }
        let ts = Transform::from_scale(scale, scale);
        let mut paint = Paint::default();
        paint.anti_alias = true;

        paint.set_color_rgba8(232, 232, 232, 255);
        if let Some(path) = rounded_rect_path(x, y, self.width, self.height, 3.0) {
            pixmap.fill_path(&path, &paint, FillRule::Winding, ts, None);
        }
        if self.disabled {
            paint.set_color_rgba8(190, 190, 190, 255);
        } else {
            paint.set_color_rgba8(118, 118, 118, 255);
        }
        if let Some(path) = rounded_rect_path(x, y, self.width, self.height, 3.0) {
            pixmap.stroke_path(
                &path,
                &paint,
                &Stroke {
                    width: 1.0,
                    ..Stroke::default()
                },
                ts,
                None,
            );
        }
    }
}
