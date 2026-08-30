//! Parsing `transform` and `transform-origin`.
//!
//! ⛔ Value PARSING, which had ended up in the file that APPLIES values.

#![allow(unused_imports)]
use std::collections::{HashMap, HashSet};
use crate::types::*;
use super::*;

/// Parse a CSS `transform` value string into a `CssTransform`.
pub fn parse_css_transform(v: &str) -> crate::types::CssTransform {
    use crate::types::{CssTransform, TransformOp};
    let mut ops = Vec::new();
    let v = v.trim();
    if v == "none" { return CssTransform::default(); }
    // Simple tokenizer: split on function calls like "translate(10px, 20px) rotate(45deg)"
    let mut rest = v;
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() { break; }
        // Find function name up to '('
        let paren_pos = match rest.find('(') {
            Some(p) => p,
            None    => break,
        };
        let func = rest[..paren_pos].trim().to_ascii_lowercase();
        let after_paren = &rest[paren_pos + 1..];
        // Find matching closing paren
        let close = after_paren.find(')').unwrap_or(after_paren.len());
        let args_str = &after_paren[..close];
        rest = if close + 1 < after_paren.len() { &after_paren[close + 1..] } else { "" };

        // Parse comma/whitespace-separated float args
        let args: Vec<f32> = args_str.split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .map(|s| {
                let s = s.trim().trim_end_matches("px").trim_end_matches("deg")
                          .trim_end_matches("rad").trim_end_matches("turn");
                s.parse::<f32>().unwrap_or(0.0)
            })
            .collect();

        let get = |i: usize, def: f32| -> f32 { *args.get(i).unwrap_or(&def) };

        match func.as_str() {
            "translate"  => ops.push(TransformOp::Translate(get(0, 0.0), get(1, 0.0))),
            "translatex" => ops.push(TransformOp::TranslateX(get(0, 0.0))),
            "translatey" => ops.push(TransformOp::TranslateY(get(0, 0.0))),
            "scale"      => ops.push(TransformOp::Scale(get(0, 1.0), get(1, get(0, 1.0)))),
            "scalex"     => ops.push(TransformOp::ScaleX(get(0, 1.0))),
            "scaley"     => ops.push(TransformOp::ScaleY(get(0, 1.0))),
            "rotate"     => ops.push(TransformOp::Rotate(get(0, 0.0))),
            "skewx"      => ops.push(TransformOp::SkewX(get(0, 0.0))),
            "skewy"      => ops.push(TransformOp::SkewY(get(0, 0.0))),
            "matrix"     => ops.push(TransformOp::Matrix(
                get(0, 1.0), get(1, 0.0), get(2, 0.0),
                get(3, 1.0), get(4, 0.0), get(5, 0.0),
            )),
            _ => {}
        }
    }
    CssTransform { ops }
}

/// Parse a CSS `transform-origin` value into (x, y) fractions (0.0..1.0).
pub fn parse_transform_origin(v: &str) -> (f32, f32) {
    let parts: Vec<&str> = v.split_whitespace().collect();
    let parse_one = |s: &str| -> f32 {
        match s {
            "left"   | "top"    => 0.0,
            "center"            => 0.5,
            "right"  | "bottom" => 1.0,
            _ if s.ends_with('%')  => s[..s.len()-1].parse::<f32>().unwrap_or(50.0) / 100.0,
            _ if s.ends_with("px") => s[..s.len()-2].parse::<f32>().unwrap_or(0.0),
            _ => s.parse::<f32>().unwrap_or(0.5),
        }
    };
    let x = parts.first().map(|s| parse_one(s)).unwrap_or(0.5);
    let y = parts.get(1).map(|s| parse_one(s)).unwrap_or(0.5);
    (x, y)
}
