pub mod block;
pub mod inline_layout;
pub mod text;
pub mod flex;
pub mod grid;
pub mod table;
pub mod hit_test;

use crate::types::*;

// ─── Float Context ────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct FloatItem {
    pub rect:  Rect,
    pub side:  FloatSide,
    pub clear: f32,  // bottom of this float
}

#[derive(Debug, Clone, PartialEq)]
pub enum FloatSide { Left, Right }

impl Default for FloatSide {
    fn default() -> Self { Self::Left }
}

#[derive(Debug, Default, Clone)]
pub struct FloatContext {
    pub floats: Vec<FloatItem>,
}

impl FloatContext {
    pub fn available_width(
        &self,
        y: f32, line_h: f32,
        containing_w: f32,
        out_left: &mut f32,
        out_right: &mut f32,
    ) {
        *out_left  = 0.0;
        *out_right = containing_w;
        for f in &self.floats {
            if f.rect.y < y + line_h && f.clear > y {
                if f.side == FloatSide::Left {
                    let r = f.rect.x + f.rect.w;
                    if r > *out_left { *out_left = r; }
                } else {
                    let l = f.rect.x;
                    if l < *out_right { *out_right = l; }
                }
            }
        }
    }

    pub fn clear_y(&self, current_y: f32, clear: Clear) -> f32 {
        let mut y = current_y;
        for f in &self.floats {
            match clear {
                Clear::Left  if f.side == FloatSide::Left  => { if f.clear > y { y = f.clear; } }
                Clear::Right if f.side == FloatSide::Right => { if f.clear > y { y = f.clear; } }
                Clear::Both                                 => { if f.clear > y { y = f.clear; } }
                _ => {}
            }
        }
        y
    }

    pub fn place_float(
        &mut self,
        current_y: f32,
        float_w: f32, float_h: f32,
        containing_w: f32,
        side: FloatSide,
    ) -> Rect {
        // Find the lowest Y position where the float fits horizontally.
        let mut y = current_y;
        loop {
            let mut left = 0.0f32;
            let mut right = containing_w;
            self.available_width(y, float_h, containing_w, &mut left, &mut right);
            let available = right - left;
            if available >= float_w { break; }
            // Move past the nearest float bottom
            let next_y = self.floats.iter()
                .filter(|f| f.clear > y)
                .map(|f| f.clear)
                .fold(f32::MAX, f32::min);
            if next_y == f32::MAX { break; }
            y = next_y;
        }

        let mut left = 0.0f32;
        let mut right = containing_w;
        self.available_width(y, float_h, containing_w, &mut left, &mut right);

        let x = if side == FloatSide::Left { left } else { right - float_w };
        let rect = Rect::new(x, y, float_w, float_h);
        self.floats.push(FloatItem { rect, side, clear: y + float_h });
        rect
    }
}

// ─── Resolved box model ───────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Copy)]
pub struct ResolvedBox {
    pub margin_top:    f32,
    pub margin_right:  f32,
    pub margin_bottom: f32,
    pub margin_left:   f32,

    pub padding_top:    f32,
    pub padding_right:  f32,
    pub padding_bottom: f32,
    pub padding_left:   f32,

    pub border_top:    f32,
    pub border_right:  f32,
    pub border_bottom: f32,
    pub border_left:   f32,

    pub content_width:  Option<f32>,  // None = auto
    pub content_height: Option<f32>,
}

impl ResolvedBox {
    pub fn h_space(&self) -> f32 {
        self.margin_left + self.border_left + self.padding_left
            + self.padding_right + self.border_right + self.margin_right
    }
    pub fn v_space(&self) -> f32 {
        self.margin_top + self.border_top + self.padding_top
            + self.padding_bottom + self.border_bottom + self.margin_bottom
    }
    pub fn inner_h_space(&self) -> f32 {
        self.border_left + self.padding_left + self.padding_right + self.border_right
    }
    pub fn inner_v_space(&self) -> f32 {
        self.border_top + self.padding_top + self.padding_bottom + self.border_bottom
    }
}

pub fn resolve_box(style: &ComputedStyle, parent_font_px: f32,
                   containing_w: f32, root_font_px: f32) -> ResolvedBox {
    resolve_box_vp(style, parent_font_px, containing_w, root_font_px, 0.0, 0.0)
}

pub fn resolve_box_vp(style: &ComputedStyle, parent_font_px: f32,
                   containing_w: f32, root_font_px: f32,
                   viewport_w: f32, viewport_h: f32) -> ResolvedBox {
    let res = |l: &CssLength| l.resolve_vp(parent_font_px, containing_w, root_font_px, viewport_w, viewport_h);
    let font_px = style.font_size_px(parent_font_px, root_font_px);

    let pad_left   = res(&style.padding_left);
    let pad_right  = res(&style.padding_right);
    let pad_top    = res(&style.padding_top);
    let pad_bottom = res(&style.padding_bottom);

    let border_left   = if style.border_left_style   != BorderStyle::None { res(&style.border_left_width)   } else { 0.0 };
    let border_right  = if style.border_right_style  != BorderStyle::None { res(&style.border_right_width)  } else { 0.0 };
    let border_top    = if style.border_top_style    != BorderStyle::None { res(&style.border_top_width)    } else { 0.0 };
    let border_bottom = if style.border_bottom_style != BorderStyle::None { res(&style.border_bottom_width) } else { 0.0 };

    let content_width = if style.width.is_auto() {
        None
    } else {
        let mut w = res(&style.width).max(0.0);
        // box-sizing: border-box — subtract padding + border from declared width
        if style.box_sizing == BoxSizing::BorderBox {
            w = (w - pad_left - pad_right - border_left - border_right).max(0.0);
        }
        Some(w)
    };

    let content_height = if style.height.is_auto() {
        None
    } else {
        let mut h = res(&style.height).max(0.0);
        if style.box_sizing == BoxSizing::BorderBox {
            h = (h - pad_top - pad_bottom - border_top - border_bottom).max(0.0);
        }
        Some(h)
    };

    ResolvedBox {
        margin_top:    res(&style.margin_top),
        margin_right:  res(&style.margin_right),
        margin_bottom: res(&style.margin_bottom),
        margin_left:   res(&style.margin_left),

        padding_top:    pad_top,
        padding_right:  pad_right,
        padding_bottom: pad_bottom,
        padding_left:   pad_left,

        border_top,
        border_right,
        border_bottom,
        border_left,

        content_width,
        content_height,
    }
}

// ─── Layout Engine ────────────────────────────────────────────────────────────

pub struct LayoutEngine {
    pub root_font_px: f32,
    /// Logical viewport width (for vw units).
    pub viewport_w: f32,
    /// Logical viewport height (for vh units).
    pub viewport_h: f32,
    /// Reference to a font system for accurate measurement.
    pub font_system: Option<*mut cosmic_text::FontSystem>,
}

impl LayoutEngine {
    pub fn new() -> Self { Self { root_font_px: 16.0, viewport_w: 900.0, viewport_h: 700.0, font_system: None } }

    /// Main entry point: layout the full document.
    pub fn layout(&self, doc: &mut Document, viewport_width: f32) {
        let root_font_px = self.root_font_px;

        // Set up root geometry
        let rbox = resolve_box(&doc.root.style, root_font_px, viewport_width, root_font_px);
        let content_w = rbox.content_width.unwrap_or(viewport_width);
        doc.root.content_rect = Rect::new(0.0, 0.0, content_w, 0.0);
        doc.root.padding_rect = Rect::new(0.0, 0.0, content_w, 0.0);
        doc.root.border_rect  = Rect::new(0.0, 0.0, content_w, 0.0);
        doc.root.margin_rect  = Rect::new(0.0, 0.0, content_w, 0.0);

        self.layout_box(&mut doc.root, content_w, 0.0, 0.0, root_font_px, root_font_px);

        // Update root geometry with final height
        let h = doc.root.margin_rect.h;
        doc.root.content_rect.h = h;
        doc.root.padding_rect.h = h;
        doc.root.border_rect.h  = h;
    }

    pub fn layout_box(
        &self,
        node:       &mut HtmlBox,
        containing_w: f32,
        x:    f32,
        y:    f32,
        parent_font_px: f32,
        root_font_px:   f32,
    ) -> f32 {
        self.layout_box_with_fc(node, containing_w, x, y, parent_font_px, root_font_px, None)
    }

    pub fn layout_box_with_fc(
        &self,
        node:       &mut HtmlBox,
        containing_w: f32,
        x:    f32,
        y:    f32,
        parent_font_px: f32,
        root_font_px:   f32,
        fc:  Option<&mut FloatContext>,
    ) -> f32 {
        // Don't layout display:none
        if matches!(node.style.display, Display::None) {
            node.content_rect = Rect::default();
            node.padding_rect = Rect::default();
            node.border_rect  = Rect::default();
            node.margin_rect  = Rect::default();
            return 0.0;
        }

        let font_px = node.style.font_size_px(parent_font_px, root_font_px);
        let rbox = resolve_box(&node.style, font_px, containing_w, root_font_px);

        match node.style.display {
            Display::Flex | Display::InlineFlex => {
                flex::layout_flex(self, node, &rbox, containing_w, x, y, font_px, root_font_px)
            }
            Display::Grid | Display::InlineGrid => {
                grid::layout_grid(self, node, &rbox, containing_w, x, y, font_px, root_font_px)
            }
            Display::Table => {
                table::layout_table(self, node, &rbox, containing_w, x, y, font_px, root_font_px)
            }
            _ => {
                // Determine if children are block-level or inline-level
                if has_block_children(node) {
                    block::layout_block_with_fc(self, node, &rbox, containing_w, x, y, font_px, root_font_px, fc)
                } else {
                    inline_layout::layout_inline_block(self, node, &rbox, containing_w, x, y, font_px, root_font_px, None)
                }
            }
        }
    }

    /// Layout a box in inline context — returns (width, height, baseline).
    pub fn layout_inline(
        &self,
        node: &mut HtmlBox,
        max_w: f32,
        x: f32, y: f32,
        parent_font_px: f32,
        root_font_px:   f32,
    ) -> (f32, f32, f32) {
        let font_px = node.style.font_size_px(parent_font_px, root_font_px);
        let _rbox = resolve_box(&node.style, font_px, max_w, root_font_px);

        let h = self.layout_box(node, max_w, x, y, parent_font_px, root_font_px);
        let w = node.border_rect.w;
        let baseline = node.baseline;
        (w, h, baseline)
    }
}

// ─── Helper: does a box have any block-level children? ────────────────────────

pub fn has_block_children(node: &HtmlBox) -> bool {
    node.children.iter().any(|c|
        !matches!(c.style.display, Display::None) &&
        (c.style.is_block_level() || !matches!(c.style.float, Float::None))
    )
}

// ─── Absolute / fixed positioning pass ───────────────────────────────────────

pub fn layout_positioned(engine: &LayoutEngine, node: &mut HtmlBox,
                         containing_rect: Rect, parent_font_px: f32, root_font_px: f32) {
    let font_px = node.style.font_size_px(parent_font_px, root_font_px);
    let containing_w = containing_rect.w;
    let containing_h = containing_rect.h;

    let left_auto  = node.style.left.is_auto();
    let right_auto = node.style.right.is_auto();
    let top_auto   = node.style.top.is_auto();
    let bot_auto   = node.style.bottom.is_auto();

    // If both horizontal sides are set, we can compute width from stretch
    let constrained_w = if !left_auto && !right_auto {
        let l = node.style.left.resolve(font_px, containing_w, root_font_px);
        let r = node.style.right.resolve(font_px, containing_w, root_font_px);
        let rbox_inner = resolve_box(&node.style, font_px, containing_w, root_font_px);
        let w = (containing_w - l - r - rbox_inner.inner_h_space()).max(0.0);
        Some(w)
    } else {
        None
    };

    let constrained_h = if !top_auto && !bot_auto {
        let t = node.style.top.resolve(font_px, containing_h, root_font_px);
        let b = node.style.bottom.resolve(font_px, containing_h, root_font_px);
        let rbox_inner = resolve_box(&node.style, font_px, containing_w, root_font_px);
        let h = (containing_h - t - b - rbox_inner.inner_v_space()).max(0.0);
        Some(h)
    } else {
        None
    };

    // Layout to get natural size (or constrained size)
    let layout_w = constrained_w.unwrap_or(containing_w);
    engine.layout_box(node, layout_w, 0.0, 0.0, font_px, root_font_px);

    // Now resolve position offsets
    let rbox = resolve_box(&node.style, font_px, containing_w, root_font_px);
    let res_l = node.style.left.resolve(font_px, containing_w, root_font_px);
    let res_r = node.style.right.resolve(font_px, containing_w, root_font_px);
    let res_t = node.style.top.resolve(font_px, containing_h, root_font_px);
    let res_b = node.style.bottom.resolve(font_px, containing_h, root_font_px);

    let x = if !left_auto {
        containing_rect.x + res_l + rbox.margin_left
    } else if !right_auto {
        containing_rect.right() - res_r - node.border_rect.w - rbox.margin_right
    } else {
        node.border_rect.x
    };

    let y = if !top_auto {
        containing_rect.y + res_t + rbox.margin_top
    } else if !bot_auto {
        containing_rect.bottom() - res_b - node.border_rect.h - rbox.margin_bottom
    } else {
        node.border_rect.y
    };

    // Shift all rects to final position
    let dx = x - node.border_rect.x;
    let dy = y - node.border_rect.y;
    shift_rects(node, dx, dy);

    // If both sides set → we may need to re-layout with constrained size
    if let Some(cw) = constrained_w {
        if node.content_rect.w != cw {
            engine.layout_box(node, layout_w, x, y, font_px, root_font_px);
        }
    }
}

pub fn shift_rects(node: &mut HtmlBox, dx: f32, dy: f32) {
    node.content_rect.x += dx; node.content_rect.y += dy;
    node.padding_rect.x += dx; node.padding_rect.y += dy;
    node.border_rect.x  += dx; node.border_rect.y  += dy;
    node.margin_rect.x  += dx; node.margin_rect.y  += dy;
    for line in &mut node.line_cache {
        line.x += dx;
        line.y += dy;
    }
    for child in &mut node.children {
        shift_rects(child, dx, dy);
    }
}

impl Default for LayoutEngine {
    fn default() -> Self { Self::new() }
}
