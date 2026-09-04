//! `@font-face` declarations.

#![allow(unused_imports)]
use super::*;
use crate::types::*;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

// ─── @font-face declaration ───────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct FontFaceDecl {
    pub family: String,
    pub src: String,
    pub weight: Option<String>,
    pub style: Option<String>,
    pub stretch: Option<String>,
    pub display: Option<String>,
    pub unicode_range: Option<String>,
    pub size_adjust: Option<String>,
    pub ascent_override: Option<String>,
    pub descent_override: Option<String>,
    pub line_gap_override: Option<String>,
    pub feature_settings: Option<String>,
    pub variation_settings: Option<String>,
    pub language_override: Option<String>,
}

/// Extract a file path from a CSS url("...") or local("...") value.
pub fn extract_url_path(src: &str) -> String {
    let src = src.trim();
    // Strip url("...") or url('...')
    let inner = if let Some(s) = src.strip_prefix("url(") {
        s.trim_end_matches(')')
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
    } else if let Some(s) = src.strip_prefix("local(") {
        s.trim_end_matches(')')
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
    } else {
        src.trim_matches('"').trim_matches('\'')
    };
    inner.to_string()
}
