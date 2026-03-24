//! LayoutBox — geometry and rendering data, separated from the DOM tree.
//!
//! During the bridge period, LayoutBox data is duplicated between here and
//! HtmlBox. The layout engine writes to both. Eventually HtmlBox's geometry
//! fields will be removed and everything goes through LayoutBox.
//!
//! LayoutBox is indexed by node_id (same as DomArena) for O(1) lookup.

use crate::types::{Rect, ComputedStyle, LayoutLine, InlineRun};

/// Layout data for a single node — box model geometry + computed style.
#[derive(Clone, Debug)]
pub struct LayoutBox {
    /// Stable node identity — same as HtmlBox.node_id and DomArena NodeId.
    pub node_id: u32,

    // ── Box model geometry (set by layout pass) ──
    pub content_rect: Rect,
    pub padding_rect: Rect,
    pub border_rect:  Rect,
    pub margin_rect:  Rect,
    pub baseline:     f32,

    // ── Resolved box model (resolved from style at layout time) ──
    pub resolved_margin_top:    f32,
    pub resolved_margin_right:  f32,
    pub resolved_margin_bottom: f32,
    pub resolved_margin_left:   f32,
    pub resolved_border_top:    f32,
    pub resolved_border_right:  f32,
    pub resolved_border_bottom: f32,
    pub resolved_border_left:   f32,
    pub resolved_pad_top:       f32,
    pub resolved_pad_right:     f32,
    pub resolved_pad_bottom:    f32,
    pub resolved_pad_left:      f32,
    pub resolved_content_width: f32,

    // ── Collapsed margins ──
    pub collapsed_margin_top:    f32,
    pub collapsed_margin_bottom: f32,

    // ── Scroll state ──
    pub scroll_height: f32,
    pub scroll_width:  f32,
    pub scroll_top:    f32,
    pub scroll_left:   f32,

    // ── Computed style ──
    pub style: ComputedStyle,

    // ── Inline content cache ──
    pub line_cache:  Vec<LayoutLine>,
    pub inline_runs: Vec<InlineRun>,

    // ── Dirty tracking ──
    pub layout_dirty:          bool,
    pub last_containing_width: f32,
    pub cached_intrinsic_w:    std::cell::Cell<f32>,
}

impl LayoutBox {
    pub fn new(node_id: u32) -> Self {
        Self {
            node_id,
            content_rect: Rect::default(),
            padding_rect: Rect::default(),
            border_rect:  Rect::default(),
            margin_rect:  Rect::default(),
            baseline:     0.0,
            resolved_margin_top:    0.0,
            resolved_margin_right:  0.0,
            resolved_margin_bottom: 0.0,
            resolved_margin_left:   0.0,
            resolved_border_top:    0.0,
            resolved_border_right:  0.0,
            resolved_border_bottom: 0.0,
            resolved_border_left:   0.0,
            resolved_pad_top:       0.0,
            resolved_pad_right:     0.0,
            resolved_pad_bottom:    0.0,
            resolved_pad_left:      0.0,
            resolved_content_width: 0.0,
            collapsed_margin_top:    0.0,
            collapsed_margin_bottom: 0.0,
            scroll_height: 0.0,
            scroll_width:  0.0,
            scroll_top:    0.0,
            scroll_left:   0.0,
            style: ComputedStyle::default(),
            line_cache:  Vec::new(),
            inline_runs: Vec::new(),
            layout_dirty:          true,
            last_containing_width: 0.0,
            cached_intrinsic_w:    std::cell::Cell::new(f32::NAN),
        }
    }
}

/// Storage for all LayoutBoxes, indexed by node_id.
/// Uses a HashMap for sparse access (not all arena nodes have layout boxes —
/// e.g. display:none nodes don't need one).
#[derive(Clone, Debug, Default)]
pub struct LayoutStore {
    boxes: std::collections::HashMap<u32, LayoutBox>,
}

impl LayoutStore {
    pub fn new() -> Self {
        Self { boxes: std::collections::HashMap::new() }
    }

    /// Get or create a LayoutBox for a node.
    pub fn get_or_create(&mut self, node_id: u32) -> &mut LayoutBox {
        self.boxes.entry(node_id).or_insert_with(|| LayoutBox::new(node_id))
    }

    /// Get an existing LayoutBox.
    pub fn get(&self, node_id: u32) -> Option<&LayoutBox> {
        self.boxes.get(&node_id)
    }

    /// Get a mutable LayoutBox.
    pub fn get_mut(&mut self, node_id: u32) -> Option<&mut LayoutBox> {
        self.boxes.get_mut(&node_id)
    }

    /// Check if a node has a LayoutBox.
    pub fn contains(&self, node_id: u32) -> bool {
        self.boxes.contains_key(&node_id)
    }

    /// Remove a LayoutBox (e.g. when a node is removed from the DOM).
    pub fn remove(&mut self, node_id: u32) {
        self.boxes.remove(&node_id);
    }

    /// Clear all layout data (e.g. on full re-layout).
    pub fn clear(&mut self) {
        self.boxes.clear();
    }

    /// Number of layout boxes.
    pub fn len(&self) -> usize {
        self.boxes.len()
    }
}
