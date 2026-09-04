//! LayoutBox — geometry and rendering data, separated from the DOM tree.
//!
//! This module re-exports `types::LayoutBox` as the core geometry struct
//! and provides `StandaloneLayoutBox` (with node_id + style) for the
//! parallel LayoutStore. Eventually WebCore's embedded LayoutBox will go
//! away and everything goes through LayoutStore.
//!
//! LayoutBox is indexed by node_id (same as DomArena) for O(1) lookup.

use crate::types::{ComputedStyle, LayoutBox as CoreLayoutBox};

/// Standalone layout data for a single node — wraps core geometry + node identity + style.
#[derive(Clone, Debug)]
pub struct LayoutBox {
    /// Stable node identity — same as WebCore.node_id and DomArena NodeId.
    pub node_id: u32,

    /// Layout geometry data (same struct embedded in WebCore.layout).
    pub layout: CoreLayoutBox,

    /// Computed style snapshot at layout time.
    pub style: ComputedStyle,
}

impl LayoutBox {
    pub fn new(node_id: u32) -> Self {
        let mut lb = CoreLayoutBox::default();
        lb.layout_dirty = true;
        Self {
            node_id,
            layout: lb,
            style: ComputedStyle::default(),
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
        Self {
            boxes: std::collections::HashMap::new(),
        }
    }

    /// Get or create a LayoutBox for a node.
    pub fn get_or_create(&mut self, node_id: u32) -> &mut LayoutBox {
        self.boxes
            .entry(node_id)
            .or_insert_with(|| LayoutBox::new(node_id))
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
