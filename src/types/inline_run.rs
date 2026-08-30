//! A run of inline content sharing one style.

#![allow(unused_imports)]
use super::*;
use std::collections::{HashMap, HashSet};
use crate::css::*;
use crate::dom::*;
use crate::html::*;

// ─── Inline Run ───────────────────────────────────────────────────────────────

/// A styled run of text within a box's text content.
#[derive(Clone, Debug)]
pub struct InlineRun {
    pub text_offset: usize,
    pub length:      usize,
    pub style:       ComputedStyle,
}
