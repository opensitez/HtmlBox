//! BiDi visual segments.

#![allow(unused_imports)]
use super::*;
use crate::css::*;
use crate::dom::*;
use crate::html::*;
use std::collections::{HashMap, HashSet};

// ─── Visual Segment (BiDi) ───────────────────────────────────────────────────

/// One visual run within a line after BiDi reordering.
/// Stored in visual order (left-to-right screen position).
#[derive(Clone, Debug, Default)]
pub struct VisualSegment {
    /// Byte offset in the full flat text string.
    pub logical_start: usize,
    /// Byte length of this segment.
    pub length: usize,
    /// BiDi embedding level (odd = RTL, even = LTR).
    pub level: u8,
    /// X position filled by renderer after measuring all prior segments.
    pub x: f32,
    /// Width filled by renderer.
    pub width: f32,
}
