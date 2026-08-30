//! `@font-face` declarations.

#![allow(unused_imports)]
use std::collections::{HashMap, HashSet};
use rayon::prelude::*;
use crate::types::*;
use super::*;

// ─── @font-face declaration ───────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct FontFaceDecl {
    pub family: String,
    pub src:    String,
    pub weight: Option<String>,
    pub style:  Option<String>,
}

/// Extract a file path from a CSS url("...") or local("...") value.
pub fn extract_url_path(src: &str) -> String {
    let src = src.trim();
    // Strip url("...") or url('...')
    let inner = if let Some(s) = src.strip_prefix("url(") {
        s.trim_end_matches(')').trim().trim_matches('"').trim_matches('\'')
    } else if let Some(s) = src.strip_prefix("local(") {
        s.trim_end_matches(')').trim().trim_matches('"').trim_matches('\'')
    } else {
        src.trim_matches('"').trim_matches('\'')
    };
    inner.to_string()
}
