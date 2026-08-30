//! A laid-out line of inline content.

#![allow(unused_imports)]
use super::*;
use std::collections::{HashMap, HashSet};
use crate::css::*;
use crate::dom::*;
use crate::html::*;

// ─── Layout Line ──────────────────────────────────────────────────────────────

/// Result of line-breaking for a line in inline content.
#[derive(Clone, Debug, Default)]
pub struct LayoutLine {
    pub text_start:  usize,
    pub text_length: usize,
    pub x:     f32,
    pub y:     f32,
    pub width: f32,
    pub height:  f32,
    pub ascent:  f32,
    pub descent: f32,
    pub extra_space_per_word: f32,  // for text-align: justify
    /// X offset from `self.x` where text content actually starts.
    /// Non-zero when atomic inline items (e.g. checkbox, image) precede text on this line.
    pub text_x_offset: f32,
    /// BiDi visual segments in visual order. Empty = pure LTR, use logical order.
    pub visual_segments: Vec<VisualSegment>,
    /// Per-character-boundary x positions relative to `self.x + text_x_offset`, in logical pixels.
    pub char_x: Vec<f32>,
}
