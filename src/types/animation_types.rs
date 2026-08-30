//! CSS animation and transition types.

#![allow(unused_imports)]
use super::*;
use std::collections::{HashMap, HashSet};
use crate::css::*;
use crate::dom::*;
use crate::html::*;

// ─── CSS Animation / Transition types ─────────────────────────────────────────

/// A single keyframe stop inside a `@keyframes` block.
#[derive(Clone, Debug)]
pub struct KeyframeStop {
    /// Progress point in the animation (0.0 = `from` / `0%`, 1.0 = `to` / `100%`).
    pub offset: f32,
    /// CSS property/value pairs declared at this stop.
    pub properties: Vec<(String, String)>,
}

/// CSS easing function (timing function).
#[derive(Clone, Debug, PartialEq)]
pub enum EasingFn {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
    StepStart,
    StepEnd,
    Steps(u32, bool),  // (count, jump_start)
}
impl Default for EasingFn { fn default() -> Self { Self::Ease } }

/// CSS `animation-direction` values.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum AnimDirection { #[default] Normal, Reverse, Alternate, AlternateReverse }

/// CSS `animation-fill-mode` values.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum FillMode { #[default] None, Forwards, Backwards, Both }

/// A fully parsed CSS `animation` shorthand or sub-property group.
#[derive(Clone, Debug)]
pub struct ParsedAnimation {
    pub name:              String,
    pub duration_ms:       f32,
    pub delay_ms:          f32,
    pub timing_fn:         EasingFn,
    /// `f32::INFINITY` for `animation-iteration-count: infinite`.
    pub iteration_count:   f32,
    pub direction:         AnimDirection,
    pub fill_mode:         FillMode,
    pub play_state_paused: bool,
}

/// A fully parsed CSS `transition` shorthand or sub-property group.
#[derive(Clone, Debug)]
pub struct ParsedTransition {
    pub property:    String,
    pub duration_ms: f32,
    pub delay_ms:    f32,
    pub timing_fn:   EasingFn,
}

/// Runtime state for one active CSS animation on one element.
#[derive(Clone, Debug)]
pub struct AnimState {
    /// The WebCore raw pointer, stored as `usize` for Hash/Eq.
    pub element_id: u32,
    pub animation:  ParsedAnimation,
    pub start_time: std::time::Instant,
}

/// Runtime state for one active CSS transition on one property of one element.
#[derive(Clone, Debug)]
pub struct TransitionState {
    pub property:    String,
    pub from_value:  String,
    pub to_value:    String,
    pub start_time:  std::time::Instant,
    pub duration_ms: f32,
    pub delay_ms:    f32,
    pub timing_fn:   EasingFn,
}
