//! Display list — recorded paint commands for cached rendering.
//!
//! Instead of painting directly to a pixmap, the renderer builds a display list
//! of paint commands. The list can be cached per stacking context and only
//! rebuilt when dirty. Replaying a cached list is much faster than re-traversing
//! the box tree and re-computing geometry.
//!
//! This is an intermediate representation between the layout tree and the final
//! rasterized output. It enables:
//! - Cached stacking contexts (only repaint what changed)
//! - Hit testing by walking the list in reverse
//! - Debug/inspector visualization
//! - Future: GPU acceleration, layer compositing

use crate::types::{Rect, Color};

/// A single paint command in the display list.
#[derive(Clone, Debug)]
pub enum PaintCmd {
    /// Fill a rectangle with a solid color.
    FillRect { rect: Rect, color: Color, radius: [f32; 4] },

    /// Draw a border on a rectangle.
    Border {
        rect: Rect,
        widths: [f32; 4],  // top, right, bottom, left
        colors: [Color; 4],
        styles: [u8; 4],   // 0=none, 1=solid, 2=dashed, 3=dotted, 4=double, etc.
        radii: [f32; 4],   // top-left, top-right, bottom-right, bottom-left
    },

    /// Draw a text run at a position.
    Text {
        x: f32,
        y: f32,
        text: String,
        font_family: String,
        font_size: f32,
        font_weight: u16,
        font_style: u8,     // 0=normal, 1=italic, 2=oblique
        font_stretch: f32,  // percentage (100.0 = normal)
        line_height: f32,
        color: Color,
        decoration: TextDecoration,
        letter_spacing: f32,
        small_caps: bool,
    },

    /// Draw an image (RGBA data) at a position.
    Image {
        rect: Rect,
        data: ImageRef,
    },

    /// Push a clip rectangle — all subsequent commands are clipped to this rect.
    PushClip { rect: Rect, radius: [f32; 4] },

    /// Pop the current clip.
    PopClip,

    /// Push a CSS transform.
    PushTransform { transform: [f32; 6] }, // 2D affine: [a, b, c, d, e, f]

    /// Pop the current transform.
    PopTransform,

    /// Push opacity — all subsequent commands are rendered with this alpha.
    PushOpacity { alpha: f32 },

    /// Pop opacity.
    PopOpacity,

    /// Push a CSS filter — content rendered into a layer, filter applied on pop.
    PushFilter { filters: Vec<(u8, f32, crate::types::Color)> },
    // filter type: 0=blur, 1=brightness, 2=contrast, 3=grayscale, 4=hue-rotate,
    //              5=invert, 6=opacity, 7=saturate, 8=sepia, 9=drop-shadow

    /// Pop filter layer.
    PopFilter,

    /// Push a blend mode layer — subsequent content is composited with this mode.
    PushBlendMode { mode: u8 }, // 0=normal, 1=multiply, 2=screen, 3=overlay, etc.

    /// Pop blend mode.
    PopBlendMode,

    /// Draw a box shadow.
    BoxShadow {
        rect: Rect,
        color: Color,
        offset_x: f32,
        offset_y: f32,
        blur: f32,
        spread: f32,
        inset: bool,
        radii: [f32; 4],
    },

    /// Draw a linear or radial gradient.
    Gradient {
        rect: Rect,
        gradient_type: u8,  // 1=linear, 2=radial
        angle: f32,
        stops: Vec<(Color, f32)>,  // (color, position 0..1)
        radii: [f32; 4],
        opacity: f32,
        blend_mode: u8,
    },

    /// Draw an outline (CSS outline property).
    Outline {
        rect: Rect,
        width: f32,
        color: Color,
        style: u8,  // same encoding as border styles
        offset: f32,
    },

    /// Draw a horizontal line (for <hr>).
    HorizontalRule {
        x1: f32,
        y1: f32,
        x2: f32,
    },

    /// List marker (bullet or number).
    ListMarker {
        marker_type: u8,   // 0=disc, 1=circle, 2=square, 3=text
        x: f32,
        y: f32,
        size: f32,
        color: Color,
        text: String,       // for numbered markers
        font_family: String,
        font_size: f32,
        font_weight: u16,
        font_style: u8,
        line_height: f32,
    },

    /// Form element placeholder — the replay function handles rendering.
    FormElement {
        tag: String,
        input_type: String,
        rect: Rect,
        node_id: u32,
        attributes: Vec<(String, String)>,
        font_size: f32,
        font_weight: u16,
        font_family: String,
        color: Color,
        checked: bool,
        value: String,
        placeholder: String,
        input_cursor: usize,
    },

    /// Draw a text shadow (separate from main text for layering).
    TextShadow {
        x: f32,
        y: f32,
        text: String,
        font_family: String,
        font_size: f32,
        font_weight: u16,
        font_style: u8,
        font_stretch: f32,
        line_height: f32,
        color: Color,
        blur: f32,
    },

    /// Background image with positioning/sizing metadata.
    BackgroundImage {
        container: Rect,        // padding rect
        data: ImageRef,
        size_mode: u8,          // 0=auto, 1=cover, 2=contain, 3=explicit
        draw_w: f32,
        draw_h: f32,
        pos_x: f32,
        pos_y: f32,
        repeat_x: bool,
        repeat_y: bool,
        radii: [f32; 4],
    },

    /// Marker: start of a stacking context.
    BeginStackingContext { node_id: u32, z_index: i32 },

    /// Marker: end of a stacking context.
    EndStackingContext,
}

/// Text decoration info for a text run.
#[derive(Clone, Debug, Default)]
pub struct TextDecoration {
    pub underline: bool,
    pub overline: bool,
    pub strikethrough: bool,
    pub color: Color,
    pub style: u8,     // 0=solid, 1=double, 2=dotted, 3=dashed, 4=wavy
    pub thickness: f32,
}

/// Reference to image data — avoids cloning large pixel buffers.
#[derive(Clone, Debug)]
pub enum ImageRef {
    /// Inline RGBA data (for small images or when we need ownership).
    Owned(Vec<u8>, u32, u32),  // (rgba_data, width, height)
    /// Shared reference via Arc (for large images).
    Shared(std::sync::Arc<Vec<u8>>, u32, u32),
}

/// A display list — ordered sequence of paint commands.
#[derive(Clone, Debug, Default)]
pub struct DisplayList {
    pub commands: Vec<PaintCmd>,
}

impl DisplayList {
    pub fn new() -> Self {
        Self { commands: Vec::new() }
    }

    pub fn push(&mut self, cmd: PaintCmd) {
        self.commands.push(cmd);
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Hit test: find the deepest node_id at a point by walking the display
    /// list in reverse (last painted = topmost visual).
    pub fn hit_test(&self, x: f32, y: f32) -> Option<u32> {
        let _clip_stack: Vec<Rect> = Vec::new();
        let mut stacking_ids: Vec<u32> = Vec::new();

        // Walk forward to build clip context, then check each rect
        for cmd in self.commands.iter().rev() {
            match cmd {
                PaintCmd::EndStackingContext => {
                    // Entering a stacking context (reverse order)
                }
                PaintCmd::BeginStackingContext { node_id, .. } => {
                    stacking_ids.push(*node_id);
                }
                PaintCmd::FillRect { rect, .. } => {
                    if rect.contains(x, y) {
                        // Return the most recent stacking context node_id
                        if let Some(&id) = stacking_ids.last() {
                            if id != 0 { return Some(id); }
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }
}

/// A cached stacking context — contains a display list and dirty flag.
#[derive(Clone, Debug)]
pub struct StackingContextCache {
    pub node_id: u32,
    pub z_index: i32,
    pub list: DisplayList,
    pub dirty: bool,
}

impl StackingContextCache {
    pub fn new(node_id: u32, z_index: i32) -> Self {
        Self {
            node_id,
            z_index,
            list: DisplayList::new(),
            dirty: true,
        }
    }
}
