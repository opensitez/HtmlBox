pub mod types;
pub mod css;
pub mod html;
pub mod layout;
pub mod renderer;
pub mod platform;
pub mod dom;
pub mod markdown;
#[cfg(feature = "accessibility")]
pub mod accessibility;

#[cfg(test)]
pub mod tests;

pub use types::{Document, HtmlBox, ComputedStyle, Rect, Color};
pub use markdown::{parse_markdown, serializer::serialize_markdown};
pub use html::{parse_html, parse_html_with_base, parse_html_bytes, parse_html_bytes_with_base};
pub use layout::LayoutEngine;
pub use layout::hit_test::{HitResult, point_to_hit, offset_to_point, hit_test_box_at, hit_test_link, get_caret_x, get_offset_from_x};
pub use renderer::Renderer;
pub use dom::HtmlEventType;

/// High-level convenience: parse HTML, layout, ready to render.
pub fn load_html(html: &str, viewport_width: f32) -> Document {
    load_html_vp(html, viewport_width, 700.0)
}

/// Like `load_html` but with explicit viewport height (needed for `100vh` layouts).
pub fn load_html_vp(html: &str, viewport_width: f32, viewport_height: f32) -> Document {
    load_html_with_registry(html, viewport_width, viewport_height, types::ComponentRegistry::default())
}

/// Parse HTML and layout with custom component registry.
pub fn load_html_with_registry(
    html: &str,
    viewport_width: f32,
    viewport_height: f32,
    registry: types::ComponentRegistry,
) -> Document {
    let mut doc = parse_html(html);
    // Re-run cascade with the real viewport so @media queries (min-width, max-width, etc.)
    // are evaluated against the actual window size rather than the default vw=0, vh=0.
    let ss = doc.stylesheet.clone();
    css::apply_cascade_vp(&mut doc.root, &ss, None, 16.0, viewport_width, viewport_height, std::ptr::null());
    let mut engine = LayoutEngine::new();
    engine.viewport_w = viewport_width;
    engine.viewport_h = viewport_height;
    engine.component_registry = registry;
    engine.layout(&mut doc, viewport_width);
    // Fire DOMContentLoaded — listeners registered before load_html can react.
    let evt = dom::HtmlEvent::new(dom::HtmlEventType::DOMContentLoaded);
    doc.events.dispatch(&doc.root, evt);
    doc
}
