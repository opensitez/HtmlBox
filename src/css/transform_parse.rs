//! Parsing `transform` and `transform-origin`.
//!
//! ⛔ Value PARSING, which had ended up in the file that APPLIES values.

#![allow(unused_imports)]
use super::*;
use crate::types::*;
use std::collections::{HashMap, HashSet};

/// Parse a CSS `transform` value string into a `CssTransform`.
pub fn parse_css_transform(v: &str) -> crate::types::CssTransform {
    parse_css_transform_checked(v).unwrap_or_default()
}

/// Parse a CSS `transform` value, rejecting the whole declaration if any
/// transform function is unknown or malformed.
pub fn parse_css_transform_checked(v: &str) -> Option<crate::types::CssTransform> {
    use crate::types::{CssTransform, TransformOp};
    let mut ops = Vec::new();
    let v = v.trim();
    if v == "none" {
        return Some(CssTransform::default());
    }
    // Simple tokenizer: split on function calls like "translate(10px, 20px) rotate(45deg)"
    let mut rest = v;
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        // Find function name up to '('
        let paren_pos = match rest.find('(') {
            Some(p) => p,
            None => return None,
        };
        let func = rest[..paren_pos].trim().to_ascii_lowercase();
        let after_paren = &rest[paren_pos + 1..];
        // Find matching closing paren
        let close = after_paren.find(')')?;
        let args_str = &after_paren[..close];
        rest = if close + 1 < after_paren.len() {
            &after_paren[close + 1..]
        } else {
            ""
        };

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
        let raw: Vec<&str> = args_str
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .collect();

        // A `<length-percentage>`, kept UNRESOLVED — see `TransformOp`.
        let len = |i: usize| -> CssLength {
            match raw.get(i) {
                Some(s) => crate::css::value_parse::parse_length(s.trim()),
                None => CssLength::Px(0.0),
            }
        };
        // An `<angle>`, in degrees. CSS Values 4 §7.1: 1turn = 360deg,
        // 1rad = 180/PI deg, 1grad = 0.9deg.
        let ang = |i: usize, def: f32| -> f32 {
            let Some(s) = raw.get(i) else { return def };
            let s = s.trim();
            let lower = s.to_ascii_lowercase();
            let num = |suffix: &str| lower.trim_end_matches(suffix).parse::<f32>().unwrap_or(0.0);
            if lower.ends_with("turn") {
                num("turn") * 360.0
            } else if lower.ends_with("grad") {
                num("grad") * 0.9
            } else if lower.ends_with("rad") {
                num("rad") * 180.0 / std::f32::consts::PI
            } else if lower.ends_with("deg") {
                num("deg")
            } else {
                lower.parse::<f32>().unwrap_or(def)
            }
        };
        // A unitless `<number>`, for the scale and matrix families.
        let get = |i: usize, def: f32| -> f32 {
            raw.get(i)
                .and_then(|s| s.trim().parse::<f32>().ok())
                .unwrap_or(def)
        };

        match func.as_str() {
            "translate" => ops.push(TransformOp::Translate(len(0), len(1))),
            "translatex" => ops.push(TransformOp::TranslateX(len(0))),
            "translatey" => ops.push(TransformOp::TranslateY(len(0))),
            "translate3d" => ops.push(TransformOp::Translate(len(0), len(1))),
            "translatez" => {}
            "scale" => ops.push(TransformOp::Scale(get(0, 1.0), get(1, get(0, 1.0)))),
            "scalex" => ops.push(TransformOp::ScaleX(get(0, 1.0))),
            "scaley" => ops.push(TransformOp::ScaleY(get(0, 1.0))),
            "scale3d" => ops.push(TransformOp::Scale(get(0, 1.0), get(1, 1.0))),
            "scalez" => {}
            "rotate" => ops.push(TransformOp::Rotate(ang(0, 0.0))),
            "rotatez" => ops.push(TransformOp::Rotate(ang(0, 0.0))),
            "rotatex" | "rotatey" => {}
            "skew" => ops.push(TransformOp::SkewX(ang(0, 0.0))),
            "skewx" => ops.push(TransformOp::SkewX(ang(0, 0.0))),
            "skewy" => ops.push(TransformOp::SkewY(ang(0, 0.0))),
            "matrix" => ops.push(TransformOp::Matrix(
                get(0, 1.0),
                get(1, 0.0),
                get(2, 0.0),
                get(3, 1.0),
                get(4, 0.0),
                get(5, 0.0),
            )),
            "matrix3d" => ops.push(TransformOp::Matrix(
                get(0, 1.0),
                get(1, 0.0),
                get(4, 0.0),
                get(5, 1.0),
                get(12, 0.0),
                get(13, 0.0),
            )),
            "perspective" => {}
            _ => return None,
        }
    }
    Some(CssTransform { ops })
}

pub fn parse_individual_translate(v: &str) -> Option<crate::types::CssTransform> {
    parse_css_transform_checked(&format!("translate({})", individual_transform_value(v)?))
}

pub fn parse_individual_rotate(v: &str) -> Option<crate::types::CssTransform> {
    parse_css_transform_checked(&format!("rotate({})", individual_transform_value(v)?))
}

pub fn parse_individual_scale(v: &str) -> Option<crate::types::CssTransform> {
    parse_css_transform_checked(&format!("scale({})", individual_transform_value(v)?))
}

fn individual_transform_value(v: &str) -> Option<&str> {
    let value = v.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some("");
    }
    if value.is_empty() || value.contains('(') || value.contains(')') {
        return None;
    }
    Some(value)
}

/// Parse a CSS `transform-origin` into a pair of lengths from the reference
/// box's top-left corner — css-transforms-1 §transform-origin.
///
/// ⛔ NOT a 0..1 fraction. A percentage IS a fraction of the box, but a
/// `<length>` is a fixed offset: this returned the raw number for `px`, which
/// the matrix builder then multiplied by the box size, so `transform-origin:
/// 10px 10px` on a 200px box put the origin 2000px outside it — a `rotate()`
/// flung the element off-screen. Keeping the `CssLength` makes both kinds
/// resolve correctly against the same containing size (the box's own).
pub fn parse_transform_origin(v: &str) -> (CssLength, CssLength) {
    // css-transforms-1 §transform-origin: `left`/`right` name the HORIZONTAL
    // axis and `top`/`bottom` the vertical one whichever position they appear
    // in, while a length or `center` is positional — x first, then y. Treating
    // every first token as x made `transform-origin: top` mean `left`.
    let mut x: Option<CssLength> = None;
    let mut y: Option<CssLength> = None;
    let mut positional: Vec<CssLength> = Vec::new();
    // A third component is the z offset, which this engine does not use.
    for tok in v.split_whitespace().take(2) {
        match tok.to_ascii_lowercase().as_str() {
            "left" => x = Some(CssLength::Percent(0.0)),
            "right" => x = Some(CssLength::Percent(100.0)),
            "top" => y = Some(CssLength::Percent(0.0)),
            "bottom" => y = Some(CssLength::Percent(100.0)),
            "center" => positional.push(CssLength::Percent(50.0)),
            _ => positional.push(match crate::css::value_parse::parse_length_checked(tok) {
                Some(l) if !l.is_auto() => l,
                _ => CssLength::Percent(50.0),
            }),
        }
    }
    for p in positional {
        if x.is_none() {
            x = Some(p);
        } else if y.is_none() {
            y = Some(p);
        }
    }
    (
        x.unwrap_or(CssLength::Percent(50.0)),
        y.unwrap_or(CssLength::Percent(50.0)),
    )
}
