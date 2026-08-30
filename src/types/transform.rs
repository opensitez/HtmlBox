//! `transform` values.

#![allow(unused_imports)]
use super::*;
use std::collections::{HashMap, HashSet};
use crate::css::*;
use crate::dom::*;
use crate::html::*;

// ─── CSS Transform ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct CssTransform {
    pub ops: Vec<TransformOp>,
}

#[derive(Clone, Debug)]
pub enum TransformOp {
    Translate(f32, f32),
    TranslateX(f32),
    TranslateY(f32),
    Scale(f32, f32),
    ScaleX(f32),
    ScaleY(f32),
    Rotate(f32),   // degrees
    SkewX(f32),   // degrees
    SkewY(f32),   // degrees
    Matrix(f32, f32, f32, f32, f32, f32),  // a b c d e f
}
