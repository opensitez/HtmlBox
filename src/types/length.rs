//! `CssLength` and the calc node.

#![allow(unused_imports)]
use super::*;
use std::collections::{HashMap, HashSet};
use crate::css::*;
use crate::dom::*;
use crate::html::*;

// ─── CSS Length ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum CssLength {
    Px(f32),
    Em(f32),
    Rem(f32),
    Percent(f32),
    /// Viewport-width percentage (1vw = 1% of viewport width).
    Vw(f32),
    /// Viewport-height percentage (1vh = 1% of viewport height).
    Vh(f32),
    // ── The four rare variants below are BOXED, and the reason is size ──
    // `CssLength` appears 53 times in `ComputedStyle`, so its width dominates:
    // an inline `Calc([f32; 6])` (24 bytes) or a three-Box `Clamp` (24 bytes)
    // made every length 32 bytes and `ComputedStyle` 3352. Every element owns
    // one, so a 100k-node page carried ~335 MB of style — and the cascade
    // recurses with several of them live per frame, which is what limited
    // nesting depth. Boxing the rare shapes costs one allocation on the few
    // lengths that use them and takes the common ones to 16 bytes.
    /// `calc()` — linear combination [percent, px, em, rem, vw, vh].
    Calc(Box<[f32; 6]>),
    /// `calc()` with non-linear parts (min/max nested inside calc).
    CalcExpr(Box<CalcNode>),
    /// `min()` — resolves to the smallest value.
    Min(Box<Vec<CssLength>>),
    /// `max()` — resolves to the largest value.
    Max(Box<Vec<CssLength>>),
    /// `clamp(min, val, max)` — resolves to val clamped between min and max.
    Clamp(Box<[CssLength; 3]>),
    Auto,
    Zero,
    None,
}

/// Expression node for calc() with nested min/max/clamp.
#[derive(Clone, Debug, PartialEq)]
pub enum CalcNode {
    Value(CssLength),
    Add(Box<CalcNode>, Box<CalcNode>),
    Sub(Box<CalcNode>, Box<CalcNode>),
    Mul(Box<CalcNode>, f32),
    Div(Box<CalcNode>, f32),
}

impl CalcNode {
    pub fn resolve_vp(&self, parent_font_px: f32, containing_px: f32, root_font_px: f32, vw: f32, vh: f32) -> f32 {
        match self {
            CalcNode::Value(v) => v.resolve_vp(parent_font_px, containing_px, root_font_px, vw, vh),
            CalcNode::Add(a, b) => a.resolve_vp(parent_font_px, containing_px, root_font_px, vw, vh)
                                 + b.resolve_vp(parent_font_px, containing_px, root_font_px, vw, vh),
            CalcNode::Sub(a, b) => a.resolve_vp(parent_font_px, containing_px, root_font_px, vw, vh)
                                 - b.resolve_vp(parent_font_px, containing_px, root_font_px, vw, vh),
            CalcNode::Mul(a, f) => a.resolve_vp(parent_font_px, containing_px, root_font_px, vw, vh) * f,
            CalcNode::Div(a, f) => if *f != 0.0 { a.resolve_vp(parent_font_px, containing_px, root_font_px, vw, vh) / f } else { 0.0 },
        }
    }
}

impl Default for CssLength {
    fn default() -> Self { Self::Auto }
}

impl CssLength {
    pub fn resolve(&self, parent_font_px: f32, containing_px: f32, root_font_px: f32) -> f32 {
        self.resolve_vp(parent_font_px, containing_px, root_font_px, 0.0, 0.0)
    }

    /// Resolve with explicit viewport dimensions for `vw`/`vh`.
    pub fn resolve_vp(
        &self,
        parent_font_px: f32,
        containing_px:  f32,
        root_font_px:   f32,
        viewport_w:     f32,
        viewport_h:     f32,
    ) -> f32 {
        match self {
            CssLength::Px(v)      => *v,
            CssLength::Em(v)      => v * parent_font_px,
            CssLength::Rem(v)     => v * root_font_px,
            CssLength::Percent(v) => v / 100.0 * containing_px,
            CssLength::Vw(v)      => v / 100.0 * viewport_w,
            CssLength::Vh(v)      => v / 100.0 * viewport_h,
            CssLength::Calc(c) =>
                c[0] / 100.0 * containing_px + c[1] + c[2] * parent_font_px
                + c[3] * root_font_px + c[4] / 100.0 * viewport_w + c[5] / 100.0 * viewport_h,
            CssLength::CalcExpr(node) =>
                node.resolve_vp(parent_font_px, containing_px, root_font_px, viewport_w, viewport_h),
            CssLength::Min(vals) => vals.iter()
                .map(|v| v.resolve_vp(parent_font_px, containing_px, root_font_px, viewport_w, viewport_h))
                .fold(f32::INFINITY, f32::min),
            CssLength::Max(vals) => vals.iter()
                .map(|v| v.resolve_vp(parent_font_px, containing_px, root_font_px, viewport_w, viewport_h))
                .fold(f32::NEG_INFINITY, f32::max),
            CssLength::Clamp(parts) => {
                let (min, val, max) = (&parts[0], &parts[1], &parts[2]);
                let min_v = min.resolve_vp(parent_font_px, containing_px, root_font_px, viewport_w, viewport_h);
                let val_v = val.resolve_vp(parent_font_px, containing_px, root_font_px, viewport_w, viewport_h);
                let max_v = max.resolve_vp(parent_font_px, containing_px, root_font_px, viewport_w, viewport_h);
                val_v.max(min_v).min(max_v)
            }
            CssLength::Auto       => 0.0,
            CssLength::Zero       => 0.0,
            CssLength::None       => 0.0,
        }
    }

    pub fn is_auto(&self) -> bool { matches!(self, CssLength::Auto) }
    pub fn is_none(&self) -> bool { matches!(self, CssLength::None) }
}
