//! `<input type=color>` — HTML §4.10.5.1.15.
//!
//! Lifted from `vybe_widgets::color_picker`, but only the CLOSED control. That
//! file is 747 lines because it also contains the picker: a hue strip, a
//! saturation/value square, hex entry and the popup that holds them. None of
//! that belongs here — a picker is user-agent chrome that appears on activation,
//! and this engine has no popup surface of its own. The closed control is what a
//! page lays out and what a screenshot shows.
//!
//! ⛔ It is NOT a text field. Falling through to the text arm rendered the value
//! as the string `#3366cc`, which is the one thing this element never displays.

use tiny_skia::{FillRule, Paint, Pixmap, Stroke, Transform};

use super::rounded_rect_path;

/// The colour well: a border and, inside it, the chosen colour.
pub struct ColorSwatch {
    pub rgba: (u8, u8, u8, u8),
    pub width: f32,
    pub height: f32,
}

/// The palette a colour picker offers.
///
/// A grid rather than a hue/saturation area: a still, clickable set of colours
/// is what a picker needs to BE useful, and the continuous picker
/// (`vybe_widgets::color_picker`'s hue strip and SV square) is a refinement on
/// top rather than a prerequisite. Eight columns of five — greys along the top
/// row, then the hues at four lightnesses.
pub const PALETTE: &[(u8, u8, u8)] = &[
    (0, 0, 0), (64, 64, 64), (128, 128, 128), (192, 192, 192),
    (224, 224, 224), (255, 255, 255), (128, 0, 0), (255, 255, 255),
    (255, 0, 0), (255, 128, 0), (255, 255, 0), (128, 255, 0),
    (0, 255, 0), (0, 255, 128), (0, 255, 255), (0, 128, 255),
    (0, 0, 255), (128, 0, 255), (255, 0, 255), (255, 0, 128),
    (192, 0, 0), (192, 96, 0), (192, 192, 0), (96, 192, 0),
    (0, 192, 0), (0, 192, 96), (0, 192, 192), (0, 96, 192),
    (0, 0, 192), (96, 0, 192), (192, 0, 192), (192, 0, 96),
    (96, 0, 0), (96, 48, 0), (96, 96, 0), (48, 96, 0),
    (0, 96, 0), (0, 96, 48), (0, 96, 96), (0, 48, 96),
];

/// How many colours sit on one row of the palette.
pub const PALETTE_COLUMNS: usize = 8;

/// The side of one palette cell, in CSS pixels.
pub const PALETTE_CELL: f32 = 18.0;

/// `#rrggbb`, the only form `<input type=color>` accepts — so this is what a
/// pick writes back into the value.
pub fn to_simple_colour(rgb: (u8, u8, u8)) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb.0, rgb.1, rgb.2)
}

impl ColorSwatch {
    pub fn new(rgba: (u8, u8, u8, u8)) -> Self {
        Self {
            rgba,
            width: 44.0,
            height: 24.0,
        }
    }

    /// Parse the element's value, which HTML requires to be a **valid simple
    /// colour**: exactly `#` followed by six ASCII hex digits, lowercase on
    /// read-back. Anything else is invalid and the value defaults to black —
    /// the spec is unusually strict here precisely so a control never has to
    /// guess.
    pub fn parse(value: &str) -> (u8, u8, u8, u8) {
        let v = value.trim();
        let hex = match v.strip_prefix('#') {
            Some(h) if h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()) => h,
            // "the value must be a valid simple colour" — otherwise black.
            _ => return (0, 0, 0, 255),
        };
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
        (byte(0), byte(2), byte(4), 255)
    }

    pub fn paint(&self, pixmap: &mut Pixmap, x: f32, y: f32, scale: f32) {
        if self.width <= 0.0 || self.height <= 0.0 {
            return;
        }
        let ts = Transform::from_scale(scale, scale);
        let mut paint = Paint::default();
        paint.anti_alias = true;

        // The well: the swatch is INSET from the border, as every browser draws
        // it, so a white or very light colour is still visibly a swatch rather
        // than an empty box.
        let inset = 3.0_f32.min(self.width / 4.0).min(self.height / 4.0);
        let (r, g, b, a) = self.rgba;
        paint.set_color_rgba8(r, g, b, a);
        if let Some(path) = rounded_rect_path(
            x + inset,
            y + inset,
            (self.width - inset * 2.0).max(0.0),
            (self.height - inset * 2.0).max(0.0),
            1.0,
        ) {
            pixmap.fill_path(&path, &paint, FillRule::Winding, ts, None);
        }

        paint.set_color_rgba8(118, 118, 118, 255);
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        if let Some(path) = rounded_rect_path(x, y, self.width, self.height, 3.0) {
            pixmap.stroke_path(&path, &paint, &stroke, ts, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ColorSwatch;

    #[test]
    fn only_a_valid_simple_colour_is_accepted() {
        assert_eq!(ColorSwatch::parse("#3366cc"), (0x33, 0x66, 0xcc, 255));
        assert_eq!(ColorSwatch::parse("#FFAA00"), (0xff, 0xaa, 0x00, 255));
        // HTML §4.10.5.1.15 takes ONLY `#rrggbb`. Everything else is invalid
        // and the value is black — named colours and the three-digit form
        // included, however reasonable they look.
        for invalid in ["red", "#fff", "3366cc", "", "#3366cg", "#3366cc1"] {
            assert_eq!(
                ColorSwatch::parse(invalid),
                (0, 0, 0, 255),
                "{invalid:?} is not a valid simple colour"
            );
        }
    }
}
