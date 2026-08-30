//! The serialization entry points.

#![allow(unused_imports)]
use super::*;
use crate::types::*;
use crate::css::*;

// ─── Serialization ───────────────────────────────────────────────────────────

pub fn serialize_html(doc: &Document) -> String {
    serializer::serialize_html(doc)
}
