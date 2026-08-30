//! Host-registered custom components.

#![allow(unused_imports)]
use super::*;
use std::collections::{HashMap, HashSet};
use crate::css::*;
use crate::dom::*;
use crate::html::*;

// ─── Custom Components ───────────────────────────────────────────────────────

pub type ComponentMeasureFn = Arc<dyn Fn(&WebCore, f32) -> (f32, f32) + Send + Sync>;
pub type ComponentPaintFn   = Arc<dyn Fn(&WebCore, &mut tiny_skia::Pixmap, f32, f32, f32, f32, f32) + Send + Sync>;

#[derive(Clone)]
pub struct ComponentCallbacks {
    pub measure: ComponentMeasureFn,
    pub paint:   ComponentPaintFn,
}

/// A custom component that fully participates in the layout pipeline.
///
/// App developers implement this trait to create custom elements that work
/// as first-class citizens in the engine — like Flutter's `RenderObject`.
///
/// Register with `engine.register_component("my-widget", MyWidget::factory)`.
/// Use in HTML: `<my-widget data-foo="bar" style="width:200px"/>`.
///
/// # Example
/// ```ignore
/// struct ProgressBar;
/// impl Component for ProgressBar {
///     fn measure(&self, node: &WebCore, available_width: f32) -> (f32, f32) {
///         let pct = node.attributes.get("value")
///             .and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
///         (available_width, 24.0) // full width, 24px tall
///     }
///     fn paint(&self, node: &WebCore, pixmap: &mut tiny_skia::Pixmap,
///              x: f32, y: f32, w: f32, h: f32, scale: f32) {
///         // draw track + filled portion
///     }
///     fn intrinsic_width(&self, _node: &WebCore) -> (f32, f32) {
///         (100.0, 300.0) // min 100px, preferred 300px
///     }
/// }
/// ```
pub trait Component: Send + Sync {
    /// Measure: given available width, return (content_width, content_height).
    fn measure(&self, node: &WebCore, available_width: f32) -> (f32, f32);

    /// Paint: draw the component into the pixmap at the given position.
    fn paint(&self, node: &WebCore, pixmap: &mut tiny_skia::Pixmap,
             x: f32, y: f32, w: f32, h: f32, scale: f32);

    /// Intrinsic sizes for parent flex/grid/table sizing.
    /// Returns (min_content_width, max_content_width).
    /// Default: uses measure() with width=0 for min, width=infinity for max.
    fn intrinsic_width(&self, node: &WebCore) -> (f32, f32) {
        let (min_w, _) = self.measure(node, 0.0);
        let (max_w, _) = self.measure(node, f32::MAX);
        (min_w, max_w)
    }

    /// Hit test: is the given point (relative to the component's content rect) inside?
    /// Default: rectangular hit test (always true if within bounds).
    fn hit_test(&self, _node: &WebCore, _x: f32, _y: f32) -> bool { true }

    /// Handle an input event. Return true if the event was consumed.
    fn handle_event(&self, _node: &mut WebCore, _event: &ComponentEvent) -> bool { false }

    /// Accessibility role for this component.
    fn accessibility_role(&self) -> &str { "generic" }

    /// Accessibility label (human-readable name).
    fn accessibility_label(&self, _node: &WebCore) -> Option<String> { None }
}

/// Input events delivered to components.
#[derive(Debug, Clone)]
pub enum ComponentEvent {
    Click { x: f32, y: f32, button: u8 },
    MouseDown { x: f32, y: f32, button: u8 },
    MouseUp { x: f32, y: f32, button: u8 },
    MouseMove { x: f32, y: f32 },
    KeyDown { key: String, modifiers: u8 },
    KeyUp { key: String, modifiers: u8 },
    TextInput { text: String },
    Focus,
    Blur,
}

/// Factory function type for creating Component instances.
pub type ComponentFactory = Arc<dyn Fn() -> Box<dyn Component> + Send + Sync>;

#[derive(Default, Clone)]
pub struct ComponentRegistry {
    /// Legacy callback-based components
    pub map: HashMap<String, ComponentCallbacks>,
    /// Trait-based components (new API)
    pub components: HashMap<String, Arc<dyn Component>>,
}

impl ComponentRegistry {
    pub fn new() -> Self { Self::default() }

    /// Register a legacy callback-based component.
    pub fn register(&mut self, tag: &str, measure: ComponentMeasureFn, paint: ComponentPaintFn) {
        self.map.insert(tag.to_string(), ComponentCallbacks { measure, paint });
    }

    /// Register a trait-based component (new API).
    /// The component instance is shared across all elements with this tag.
    pub fn register_component(&mut self, tag: &str, component: impl Component + 'static) {
        self.components.insert(tag.to_string(), Arc::new(component));
    }

    /// Register a component from an Arc (for sharing across threads).
    pub fn register_component_arc(&mut self, tag: &str, component: Arc<dyn Component>) {
        self.components.insert(tag.to_string(), component);
    }

    /// Look up a component by tag name. Checks trait-based first, then legacy.
    pub fn get_component(&self, tag: &str) -> Option<&Arc<dyn Component>> {
        self.components.get(tag)
    }
}
