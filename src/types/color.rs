//! `Color`.

#![allow(unused_imports)]
use super::*;
use std::collections::{HashMap, HashSet};
use crate::css::*;
use crate::dom::*;
use crate::html::*;

// ─── Color ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self { Self { r, g, b, a } }
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self { Self { r, g, b, a: 255 } }
    pub const BLACK:       Self = Self::rgb(0, 0, 0);
    pub const WHITE:       Self = Self::rgb(255, 255, 255);
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);

    pub fn to_tiny_skia(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.r, self.g, self.b, self.a)
    }
}

impl Default for Color {
    fn default() -> Self { Self::BLACK }
}
