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

        // ⛔ The arguments are kept as STRINGS and converted per FUNCTION,
        // because a transform's arguments are not all the same kind of value.
        // This stripped `px|deg|rad|turn` off every argument and parsed what
        // was left, which was wrong twice over:
        //
        //  * a LENGTH in any other unit failed to parse and became 0, so
        //    `translateX(2rem)` and `translateX(1in)` moved nothing;
        //  * an ANGLE had its unit REMOVED rather than converted, so
        //    `rotate(1turn)` was read as 1 DEGREE instead of 360, and
        //    `rotate(1rad)` as 1 degree instead of 57.3.
        let raw: Vec<&str> = args_str.split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .collect();

        // A `<length>`, in px, through the one unit definition.
        let len = |i: usize, def: f32| -> f32 {
            match raw.get(i) {
                Some(s) => crate::css::value_parse::parse_length(s.trim())
                    .resolve_vp(16.0, 0.0, 16.0, 0.0, 0.0),
                None => def,
            }
        };
        // An `<angle>`, in degrees. CSS Values 4 §7.1: 1turn = 360deg,
        // 1rad = 180/PI deg, 1grad = 0.9deg.
        let ang = |i: usize, def: f32| -> f32 {
            let Some(s) = raw.get(i) else { return def };
            let s = s.trim();
            let lower = s.to_ascii_lowercase();
            let num = |suffix: &str| lower.trim_end_matches(suffix).parse::<f32>().unwrap_or(0.0);
            if lower.ends_with("turn") { num("turn") * 360.0 }
            else if lower.ends_with("grad") { num("grad") * 0.9 }
            else if lower.ends_with("rad") { num("rad") * 180.0 / std::f32::consts::PI }
            else if lower.ends_with("deg") { num("deg") }
            else { lower.parse::<f32>().unwrap_or(def) }
        };
        // A unitless `<number>`, for the scale and matrix families.
        let get = |i: usize, def: f32| -> f32 {
            raw.get(i).and_then(|s| s.trim().parse::<f32>().ok()).unwrap_or(def)
        };

        match func.as_str() {
            "translate"  => ops.push(TransformOp::Translate(len(0, 0.0), len(1, 0.0))),
            "translatex" => ops.push(TransformOp::TranslateX(len(0, 0.0))),
            "translatey" => ops.push(TransformOp::TranslateY(len(0, 0.0))),
            "scale"      => ops.push(TransformOp::Scale(get(0, 1.0), get(1, get(0, 1.0)))),
            "scalex"     => ops.push(TransformOp::ScaleX(get(0, 1.0))),
            "scaley"     => ops.push(TransformOp::ScaleY(get(0, 1.0))),
            "rotate"     => ops.push(TransformOp::Rotate(ang(0, 0.0))),
            "skewx"      => ops.push(TransformOp::SkewX(ang(0, 0.0))),
            "skewy"      => ops.push(TransformOp::SkewY(ang(0, 0.0))),
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
