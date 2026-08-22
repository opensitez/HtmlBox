//! Text input widget — standalone tiny-skia rendered text field.

use super::WidgetColors;

pub struct TextInput {
    pub value: String,
    pub placeholder: String,
    pub cursor: usize,
    pub password: bool,
    pub disabled: bool,
    pub focused: bool,
    pub colors: WidgetColors,
    pub font_size: f32,
    pub width: f32,
    pub height: f32,
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            placeholder: String::new(),
            cursor: 0,
            password: false,
            disabled: false,
            focused: false,
            colors: WidgetColors::default(),
            font_size: 14.0,
            width: 200.0,
            height: 24.0,
        }
    }

    pub fn with_placeholder(mut self, p: &str) -> Self { self.placeholder = p.to_string(); self }
    pub fn with_password(mut self) -> Self { self.password = true; self }

    /// Paint renders the border only. Text rendering requires a font system
    /// and is handled by the caller (browser engine uses cosmic_text,
    /// standalone apps can use any text renderer).
    pub fn paint_border(&self, pixmap: &mut tiny_skia::Pixmap, x: f32, y: f32, scale: f32) {
        let ts = tiny_skia::Transform::from_scale(scale, scale);
        let mut paint = tiny_skia::Paint::default();
        paint.anti_alias = true;

        // Background
        let (r, g, b, a) = self.colors.background;
        paint.set_color_rgba8(r, g, b, a);
        if let Some(path) = super::rounded_rect_path(x, y, self.width, self.height, 3.0) {
            pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, ts, None);
        }

        // Border
        let (r, g, b, a) = if self.focused { self.colors.focus_ring } else { self.colors.border };
        paint.set_color_rgba8(r, g, b, a);
        let mut stroke = tiny_skia::Stroke::default();
        stroke.width = if self.focused { 2.0 } else { 1.0 };
        if let Some(path) = super::rounded_rect_path(x, y, self.width, self.height, 3.0) {
            pixmap.stroke_path(&path, &paint, &stroke, ts, None);
        }
    }

    /// Get the display text (masked for password fields).
    pub fn display_text(&self) -> String {
        if self.password {
            "\u{2022}".repeat(self.value.len())
        } else if self.value.is_empty() {
            self.placeholder.clone()
        } else {
            self.value.clone()
        }
    }

    /// Is displaying placeholder text?
    pub fn is_placeholder(&self) -> bool {
        self.value.is_empty() && !self.placeholder.is_empty()
    }

    pub fn measure(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    /// Insert text at cursor position.
    pub fn insert(&mut self, text: &str) {
        if self.disabled { return; }
        let byte_pos = self.cursor.min(self.value.len());
        self.value.insert_str(byte_pos, text);
        self.cursor = byte_pos + text.len();
    }

    /// Delete character before cursor (backspace).
    pub fn backspace(&mut self) {
        if self.disabled || self.cursor == 0 { return; }
        let mut idx = self.cursor - 1;
        while idx > 0 && !self.value.is_char_boundary(idx) { idx -= 1; }
        self.value.drain(idx..self.cursor);
        self.cursor = idx;
    }

    /// Delete character after cursor.
    pub fn delete(&mut self) {
        if self.disabled || self.cursor >= self.value.len() { return; }
        let end = self.cursor + 1;
        let end = if end <= self.value.len() { end } else { self.value.len() };
        self.value.drain(self.cursor..end);
    }

    /// Move cursor left.
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            while self.cursor > 0 && !self.value.is_char_boundary(self.cursor) {
                self.cursor -= 1;
            }
        }
    }

    /// Move cursor right.
    pub fn move_right(&mut self) {
        if self.cursor < self.value.len() {
            self.cursor += 1;
            while self.cursor < self.value.len() && !self.value.is_char_boundary(self.cursor) {
                self.cursor += 1;
            }
        }
    }
}
