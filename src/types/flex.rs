//! Flex container and item enums.

#![allow(unused_imports)]
use super::*;
use std::collections::{HashMap, HashSet};
use crate::css::*;
use crate::dom::*;
use crate::html::*;

// ─── Flex / Grid ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl Default for FlexDirection {
    fn default() -> Self { Self::Row }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlexWrap {
    Nowrap,
    Wrap,
    WrapReverse,
}

impl Default for FlexWrap {
    fn default() -> Self { Self::Nowrap }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AlignItems {
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
}

impl Default for AlignItems {
    fn default() -> Self { Self::Stretch }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AlignSelf {
    Auto,
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
}

impl Default for AlignSelf {
    fn default() -> Self { Self::Auto }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JustifyContent {
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl Default for JustifyContent {
    fn default() -> Self { Self::FlexStart }
}
