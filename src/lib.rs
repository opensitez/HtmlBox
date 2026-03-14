pub mod types;
pub mod css;
pub mod html;
pub mod layout;
pub mod renderer;
pub mod platform;
pub mod dom;

#[cfg(test)]
pub mod tests;

pub use types::{Document, HtmlBox, ComputedStyle, Rect, Color};
pub use html::{parse_html, parse_html_with_base, parse_html_bytes, parse_html_bytes_with_base};
pub use layout::LayoutEngine;
pub use layout::hit_test::{HitResult, point_to_hit, offset_to_point, hit_test_box_at, hit_test_link, get_caret_x, get_offset_from_x};
pub use renderer::Renderer;

/// High-level convenience: parse HTML, layout, ready to render.
pub fn load_html(html: &str, viewport_width: f32) -> Document {
    let mut doc = parse_html(html);
    let engine  = LayoutEngine::new();
    engine.layout(&mut doc, viewport_width);
    doc
}
