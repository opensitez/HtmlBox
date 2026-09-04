//! `CssValue` — a pre-parsed declaration value.

#![allow(unused_imports)]
use super::*;
use crate::css::*;
use crate::dom::*;
use crate::html::*;
use std::collections::{HashMap, HashSet};

// ─── CSS Value (pre-parsed declaration value) ───────────────────────────────

/// Pre-parsed CSS declaration value. Produced during stylesheet compilation
/// so the cascade never re-parses strings. The `Raw` variant is the fallback
/// for values that haven't been converted to typed form yet, or for values
/// containing `var()` references that must be resolved at cascade time.
#[derive(Clone, Debug)]
pub enum CssValue {
    /// A pre-parsed length value (px, em, %, calc, min, max, clamp, auto, etc.)
    Length(CssLength),
    /// A pre-parsed color value.
    Color(Color),
    /// A numeric value (opacity, flex-grow, flex-shrink, etc.)
    Number(f32),
    /// An integer value (z-index, order, column-count, etc.)
    Integer(i32),
    /// Pre-parsed keyword enums — avoids string matching during cascade.
    Display(Display),
    Position(Position),
    Float(Float),
    Clear(Clear),
    BoxSizing(BoxSizing),
    Overflow(Overflow),
    /// visibility: true=visible, false=hidden
    Visible(bool),
    TextAlign(TextAlign),
    TextTransform(TextTransform),
    WhiteSpace(WhiteSpace),
    FontWeight(FontWeight),
    FontStyle(FontStyle),
    FlexDirection(FlexDirection),
    FlexWrap(FlexWrap),
    AlignItems(AlignItems),
    AlignSelf(AlignSelf),
    AlignContent(AlignContent),
    JustifyContent(JustifyContent),
    ListStyleType(ListStyleType),
    ListStylePosition(ListStylePosition),
    WordBreak(WordBreak),
    BorderStyle(BorderStyleValue),
    VerticalAlign(VerticalAlign),
    /// Global CSS keyword.
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
    /// Unparsed string — fallback for complex values, var() references,
    /// and properties that haven't been converted to typed form yet.
    Raw(String),
}

/// Border-style single value (not the shorthand).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BorderStyleValue {
    None,
    Hidden,
    Solid,
    Dashed,
    Dotted,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
}

impl CssValue {
    /// Extract the raw string for var() resolution and backward-compat paths.
    /// Returns the string for Raw values, empty string for typed values
    /// (typed values don't contain var() references).
    pub fn raw_str(&self) -> &str {
        match self {
            CssValue::Raw(s) => s.as_str(),
            _ => "",
        }
    }

    /// Check if this value contains a var() reference (only possible in Raw).
    pub fn has_var(&self) -> bool {
        match self {
            CssValue::Raw(s) => s.contains("var("),
            _ => false,
        }
    }
}
