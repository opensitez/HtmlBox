pub mod block;
pub mod inline_layout;
pub mod text;
pub mod flex;
pub mod grid;
pub mod table;
pub mod hit_test;

use crate::types::*;

// ─── Font loading helpers ──────────────────────────────────────────────────────

/// WOFF2 magic bytes: `wOF2` (0x774F4632).
const WOFF2_MAGIC: [u8; 4] = [0x77, 0x4F, 0x46, 0x32];

/// Load raw font bytes into the font system, with format detection.
/// WOFF2 is detected and skipped (it requires Brotli decompression which is not
/// currently bundled; convert to TTF/OTF/WOFF1 for use with @font-face).
fn load_font_bytes(fs: &mut cosmic_text::FontSystem, data: Vec<u8>) {
    if data.starts_with(&WOFF2_MAGIC) {
        // WOFF2 uses Brotli compression. fontdb cannot decode it without an
        // external decompressor. Skip and let the font-family fallback apply.
        return;
    }
    fs.db_mut().load_font_data(data);
}

/// Minimal Base64 decoder (no external dependency).
/// Returns `Err` on invalid input.
fn decode_base64(s: &str) -> Result<Vec<u8>, ()> {
    const TABLE: &[u8; 128] = b"\
        \xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\
        \xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\
        \xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\x3e\xff\xff\xff\x3f\
        \x34\x35\x36\x37\x38\x39\x3a\x3b\x3c\x3d\xff\xff\xff\xff\xff\xff\
        \xff\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\
        \x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\xff\xff\xff\xff\xff\
        \xff\x1a\x1b\x1c\x1d\x1e\x1f\x20\x21\x22\x23\x24\x25\x26\x27\x28\
        \x29\x2a\x2b\x2c\x2d\x2e\x2f\x30\x31\x32\x33\xff\xff\xff\xff\xff";

    let s: Vec<u8> = s.bytes().filter(|&b| b != b'\n' && b != b'\r' && b != b' ').collect();
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut i = 0;
    while i + 3 < s.len() {
        let a = s[i];
        let b = s[i + 1];
        let c = s[i + 2];
        let d = s[i + 3];
        if a >= 128 || b >= 128 || c >= 128 || d >= 128 { return Err(()); }
        let va = TABLE[a as usize];
        let vb = TABLE[b as usize];
        let vc = if c == b'=' { 0 } else { TABLE[c as usize] };
        let vd = if d == b'=' { 0 } else { TABLE[d as usize] };
        if va == 0xff || vb == 0xff || vc == 0xff || vd == 0xff { return Err(()); }
        out.push((va << 2) | (vb >> 4));
        if c != b'=' { out.push((vb << 4) | (vc >> 2)); }
        if d != b'=' { out.push((vc << 6) | vd); }
        i += 4;
    }
    Ok(out)
}

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
    pub origin_y: f32, // Document Y of the context root
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
    let _font_px = style.font_size_px(parent_font_px, root_font_px);

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
    /// Custom component registry for custom tags
    pub component_registry: ComponentRegistry,
    /// Device pixel ratio (e.g. 2.0 on HiDPI/Retina). Used so that char_x
    /// positions are shaped at physical pixel size — matching the renderer —
    /// giving accurate click↔caret mapping on every display density.
    pub scale: f32,
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self {
            root_font_px: 16.0,
            viewport_w: 900.0,
            viewport_h: 700.0,
            font_system: None,
            component_registry: ComponentRegistry::default(),
            scale: 1.0,
        }
    }

    /// Resolve a box's styles using the engine's viewport dimensions.
    #[inline]
    pub fn res_box(&self, style: &ComputedStyle, font_px: f32, containing_w: f32, root_font_px: f32) -> ResolvedBox {
        resolve_box_vp(style, font_px, containing_w, root_font_px, self.viewport_w, self.viewport_h)
    }

    /// Resolve a single CSS length using the engine's viewport dimensions.
    #[inline]
    pub fn res_len(&self, len: &CssLength, font_px: f32, containing: f32, root_font_px: f32) -> f32 {
        len.resolve_vp(font_px, containing, root_font_px, self.viewport_w, self.viewport_h)
    }

    /// Load custom fonts declared by @font-face rules into the font system.
    pub fn load_font_faces(&mut self, faces: &[crate::css::FontFaceDecl]) {
        if let Some(fs_ptr) = self.font_system {
            let fs = unsafe { &mut *fs_ptr };
            for face in faces {
                let src = face.src.trim();

                // ── Base64 data URI: `url("data:font/...;base64,<data>")` ──────
                let url_inner = {
                    let s = src.trim_start_matches("url(")
                               .trim_end_matches(')')
                               .trim()
                               .trim_matches('"')
                               .trim_matches('\'');
                    s
                };
                if let Some(b64) = url_inner.strip_prefix("data:")
                    .and_then(|s| s.find(";base64,").map(|i| &s[i + 8..]))
                {
                    if let Ok(bytes) = decode_base64(b64.trim()) {
                        load_font_bytes(fs, bytes);
                    }
                    continue;
                }

                // ── File path ──────────────────────────────────────────────────
                let path = crate::css::extract_url_path(src);
                if path.is_empty() { continue; }
                if let Ok(data) = std::fs::read(&path) {
                    load_font_bytes(fs, data);
                }
            }
        }
    }

    /// Main entry point: layout the full document.
    pub fn layout(&mut self, doc: &mut Document, viewport_width: f32) {
        self.viewport_w = viewport_width;
        // Keep viewport in doc so focus-change recascades use the correct size.
        doc.viewport_w = self.viewport_w;
        doc.viewport_h = self.viewport_h;
        let root_font_px = self.root_font_px;

        // Re-run CSS cascade with current viewport so @media queries reflect the real window size.
        let ss = doc.stylesheet.clone();
        crate::css::apply_cascade_vp(
            &mut doc.root, &ss, None, root_font_px,
            self.viewport_w, self.viewport_h, doc.focused_box, doc.keyboard_focus,
        );

        self.layout_geometry(doc, viewport_width, root_font_px);
    }

    /// Layout without re-running the CSS cascade.
    ///
    /// Use this when only text content changed (e.g. keystrokes in an editable
    /// element).  Skipping the cascade saves CSS selector matching across every
    /// element in the tree; the line-cache early-stop then skips unchanged lines.
    pub fn layout_no_cascade(&mut self, doc: &mut Document, viewport_width: f32) {
        self.viewport_w = viewport_width;
        let root_font_px = self.root_font_px;
        self.layout_geometry(doc, viewport_width, root_font_px);
    }

    fn layout_geometry(&self, doc: &mut Document, viewport_width: f32, root_font_px: f32) {
        // Set up root geometry
        let rbox = self.res_box(&doc.root.style, root_font_px, viewport_width, root_font_px);
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
        let mut rbox = resolve_box_vp(&node.style, font_px, containing_w, root_font_px, self.viewport_w, self.viewport_h);

        // Auto-margin centering (CSS 2.1 §10.3.3) — applies to any element with an
        // explicit width and at least one auto horizontal margin.  Block layout has
        // its own copy of this logic; here we handle flex/grid/table/custom.
        if let Some(content_w) = rbox.content_width {
            let left_auto  = node.style.margin_left.is_auto();
            let right_auto = node.style.margin_right.is_auto();
            if left_auto || right_auto {
                let non_margin = rbox.border_left + rbox.padding_left + content_w
                               + rbox.padding_right + rbox.border_right;
                let available  = (containing_w - non_margin).max(0.0);
                if left_auto && right_auto {
                    let ml = (available / 2.0).floor();
                    rbox.margin_left  = ml;
                    rbox.margin_right = available - ml;
                } else if left_auto {
                    rbox.margin_left  = available - rbox.margin_right;
                } else {
                    rbox.margin_right = available - rbox.margin_left;
                }
            }
        }

        // Check for custom component measurement
        if let Some(callbacks) = self.component_registry.map.get(&node.tag) {
            let (cw, ch) = (callbacks.measure)(node, containing_w);
            let final_w = if node.style.width.is_auto() { cw } else { rbox.content_width.unwrap_or(cw) };
            let final_h = if node.style.height.is_auto() { ch } else { rbox.content_height.unwrap_or(ch) };
            block::build_box_rects(node, &rbox, x + rbox.margin_left + rbox.border_left + rbox.padding_left,
                                   y + rbox.margin_top + rbox.border_top + rbox.padding_top,
                                   final_w, final_h, rbox.margin_left, rbox.margin_right);
            node.layout_dirty = false;
            return node.margin_rect.h;
        }

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
        let _rbox = self.res_box(&node.style, font_px, max_w, root_font_px);

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
        matches!(c.style.position, Position::Static | Position::Relative | Position::Sticky) &&
        c.style.is_block_level() &&
        matches!(c.style.float, Float::None)
    )
}

// ─── Absolute / fixed positioning pass ───────────────────────────────────────

pub fn layout_positioned(engine: &LayoutEngine, node: &mut HtmlBox,
                         containing_rect: Rect, parent_font_px: f32, root_font_px: f32) {
    let font_px = node.style.font_size_px(parent_font_px, root_font_px);
    // By default the containing block is the passed containing_rect. For `fixed`
    // positioned elements the containing block is the viewport (0,0, viewport_w, viewport_h).
    let mut containing_w = containing_rect.w;
    let mut containing_h = containing_rect.h;
    let mut containing_x = containing_rect.x;
    let mut containing_y = containing_rect.y;
    if node.style.position == Position::Fixed {
        containing_w = engine.viewport_w;
        containing_h = engine.viewport_h;
        containing_x = 0.0;
        containing_y = 0.0;
    }

    let left_auto  = node.style.left.is_auto();
    let right_auto = node.style.right.is_auto();
    let top_auto   = node.style.top.is_auto();
    let bot_auto   = node.style.bottom.is_auto();

    // If both horizontal sides are set, we can compute width from stretch
    let constrained_w = if !left_auto && !right_auto {
        let l = node.style.left.resolve_vp(font_px, containing_w, root_font_px, engine.viewport_w, engine.viewport_h);
        let r = node.style.right.resolve_vp(font_px, containing_w, root_font_px, engine.viewport_w, engine.viewport_h);
        let rbox_inner = resolve_box_vp(&node.style, font_px, containing_w, root_font_px, engine.viewport_w, engine.viewport_h);
        let w = (containing_w - l - r - rbox_inner.inner_h_space()).max(0.0);
        Some(w)
    } else {
        None
    };

    let constrained_h = if !top_auto && !bot_auto {
        let t = node.style.top.resolve_vp(font_px, containing_h, root_font_px, engine.viewport_w, engine.viewport_h);
        let b = node.style.bottom.resolve_vp(font_px, containing_h, root_font_px, engine.viewport_w, engine.viewport_h);
        let rbox_inner = resolve_box_vp(&node.style, font_px, containing_w, root_font_px, engine.viewport_w, engine.viewport_h);
        let h = (containing_h - t - b - rbox_inner.inner_v_space()).max(0.0);
        Some(h)
    } else {
        None
    };

    // Layout to get natural size (or constrained size)
    let layout_w = constrained_w.unwrap_or(containing_w);
    engine.layout_box(node, layout_w, 0.0, 0.0, font_px, root_font_px);

    // Shrink-to-fit: width:auto absolutely-positioned elements wrap their content
    // (CSS 2.1 §10.3.7), just like floats — but only when width is not already
    // constrained by having both left and right set.
    if constrained_w.is_none() && node.style.width.is_auto() {
        let intrinsic_w = block::compute_intrinsic_width(node);
        if intrinsic_w > 0.0 && intrinsic_w < layout_w {
            let shrink_w = intrinsic_w
                + node.resolved_pad_left  + node.resolved_pad_right
                + node.resolved_border_left + node.resolved_border_right
                + node.resolved_margin_left + node.resolved_margin_right;
            engine.layout_box(node, shrink_w, 0.0, 0.0, font_px, root_font_px);
        }
    }

    // Now resolve position offsets
    let rbox = resolve_box_vp(&node.style, font_px, containing_w, root_font_px, engine.viewport_w, engine.viewport_h);
    let res_l = node.style.left.resolve_vp(font_px, containing_w, root_font_px, engine.viewport_w, engine.viewport_h);
    let res_r = node.style.right.resolve_vp(font_px, containing_w, root_font_px, engine.viewport_w, engine.viewport_h);
    let res_t = node.style.top.resolve_vp(font_px, containing_h, root_font_px, engine.viewport_w, engine.viewport_h);
    let res_b = node.style.bottom.resolve_vp(font_px, containing_h, root_font_px, engine.viewport_w, engine.viewport_h);

    let x = if !left_auto {
        containing_x + res_l + rbox.margin_left
    } else if !right_auto {
        (containing_x + containing_w) - res_r - node.border_rect.w - rbox.margin_right
    } else {
        node.border_rect.x
    };

    let y = if !top_auto {
        containing_y + res_t + rbox.margin_top
    } else if !bot_auto {
        (containing_y + containing_h) - res_b - node.border_rect.h - rbox.margin_bottom
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

    // Apply constrained height when both top and bottom are set and height is auto.
    // Without this, inset:0 (top:0 bottom:0) leaves height at 0 because layout_box
    // has no content to fill the space.
    if let Some(ch) = constrained_h {
        if node.style.height.is_auto() && (node.content_rect.h - ch).abs() > 0.5 {
            let diff = ch - node.content_rect.h;
            node.content_rect.h += diff;
            node.padding_rect.h += diff;
            node.border_rect.h  += diff;
            node.margin_rect.h  += diff;
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
