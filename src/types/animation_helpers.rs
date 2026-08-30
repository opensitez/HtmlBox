//! Helpers for driving CSS animations.

#![allow(unused_imports)]
use super::*;
use std::collections::{HashMap, HashSet};
use crate::css::*;
use crate::dom::*;
use crate::html::*;

// ─── CSS Animation helpers ────────────────────────────────────────────────────

/// Extract the CSS properties that can participate in transitions from a style.
/// Values are serialised to `rgba(…)` or `Npx` strings for comparison/interpolation.
/// Find the node_id of the nearest ancestor of `target_id` that has a valid node_id.
/// Used when hit-test returns a node with node_id=0 (e.g. pseudo-elements, post-process nodes).
pub fn find_parent_node_id_by_id(root: &WebCore, target_id: u32) -> u32 {
    fn walk(node: &WebCore, target_id: u32) -> Option<u32> {
        for child in &node.children {
            if child.node_id == target_id {
                return if node.node_id != 0 { Some(node.node_id) } else { None };
            }
            if let Some(id) = walk(child, target_id) { return Some(id); }
        }
        None
    }
    walk(root, target_id).unwrap_or(0)
}

/// Find the node_id of an <a> element with the given href.
pub fn find_link_node_id(root: &WebCore, href: &str) -> Option<u32> {
    if root.tag == "a" && root.node_id != 0 && root.style.href == href {
        return Some(root.node_id);
    }
    for child in &root.children {
        if let Some(id) = find_link_node_id(child, href) {
            return Some(id);
        }
    }
    None
}

pub(crate) fn subtree_contains_id(node: &WebCore, target_id: u32) -> bool {
    if node.node_id == target_id { return true; }
    for child in &node.children {
        if subtree_contains_id(child, target_id) { return true; }
    }
    false
}

pub(crate) fn extract_transitionable(node: &WebCore) -> HashMap<String, String> {
    extract_transitionable_style(&node.style)
}

pub(crate) fn extract_transitionable_style(s: &ComputedStyle) -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("opacity".into(),          format!("{}", s.opacity));
    m.insert("color".into(),            color_to_rgba(s.color));
    m.insert("background-color".into(), color_to_rgba(s.background_color));
    m.insert("border-color".into(),     color_to_rgba(s.border_top_color));
    m.insert("transform".into(),        s.transform.clone());
    m.insert("font-size".into(),        format!("{}px", s.font_size_px(16.0, 16.0)));
    m
}

fn color_to_rgba(c: Color) -> String {
    format!("rgba({},{},{},{:.4})", c.r, c.g, c.b, c.a as f32 / 255.0)
}

/// Find the two surrounding keyframe stops for `t` and return interpolated properties.
pub(crate) fn interpolate_keyframe_stops(stops: &[KeyframeStop], t: f32) -> Vec<(String, String)> {
    if stops.is_empty() { return Vec::new(); }
    if stops.len() == 1 { return stops[0].properties.clone(); }

    // Find surrounding stops.
    let (from, to, local_t) = if t <= stops[0].offset {
        (&stops[0], &stops[0], 0.0f32)
    } else if t >= stops[stops.len() - 1].offset {
        let last = &stops[stops.len() - 1];
        (last, last, 1.0f32)
    } else {
        let mut fi = 0usize;
        for i in 0..stops.len() - 1 {
            if t >= stops[i].offset && t <= stops[i + 1].offset {
                fi = i; break;
            }
        }
        let ti = fi + 1;
        let range = stops[ti].offset - stops[fi].offset;
        let lt = if range > 1e-6 { (t - stops[fi].offset) / range } else { 0.0 };
        (&stops[fi], &stops[ti], lt)
    };

    let mut result = Vec::new();
    for (prop, from_val) in &from.properties {
        let to_val = to.properties.iter()
            .find(|(p, _)| p == prop)
            .map(|(_, v)| v.as_str())
            .unwrap_or(from_val.as_str());
        result.push((prop.clone(), interpolate_value(from_val, to_val, local_t)));
    }
    result
}

/// Interpolate between two CSS value strings.
/// Handles `rgba(…)` colors and strings containing numbers.
pub(crate) fn interpolate_value(from: &str, to: &str, t: f32) -> String {
    if let Some(c) = interpolate_color(from, to, t) { return c; }
    // For transforms, if one side is empty/none, synthesize the identity form.
    let (from, to) = if from.is_empty() || from == "none" {
        (transform_identity(to).into(), to.to_string())
    } else if to.is_empty() || to == "none" {
        (from.to_string(), transform_identity(from).into())
    } else {
        (from.to_string(), to.to_string())
    };
    interpolate_numeric(&from, &to, t)
}

/// Given a CSS transform string like `rotate(180deg)`, return the identity form
/// with the same function and matching zero-ish arguments: `rotate(0deg)`.
fn transform_identity(transform: &str) -> String {
    let s = transform.trim();
    if s.is_empty() || s == "none" { return String::new(); }
    // Find the function name and argument count.
    if let Some(open) = s.find('(') {
        let func = &s[..open];
        let inner = s[open+1..].trim_end_matches(')');
        let arg_count = inner.split(',').count();
        // scale identity is 1, everything else is 0.
        let identity_val = if func.starts_with("scale") { "1" } else { "0" };
        // Preserve units from the original arguments.
        let units: Vec<&str> = inner.split(',').map(|a| {
            let a = a.trim();
            // Strip leading minus/digits/dot to find the unit suffix.
            let num_end = a.bytes().position(|b| b.is_ascii_alphabetic())
                .unwrap_or(a.len());
            &a[num_end..]
        }).collect();
        let args: Vec<String> = (0..arg_count).map(|i| {
            format!("{}{}", identity_val, units.get(i).unwrap_or(&""))
        }).collect();
        format!("{}({})", func, args.join(", "))
    } else {
        String::new()
    }
}

fn interpolate_color(from: &str, to: &str, t: f32) -> Option<String> {
    let (fr, fg, fb, fa) = parse_rgba(from)?;
    let (tr, tg, tb, ta) = parse_rgba(to)?;
    let r = lerp(fr, tr, t).round() as u8;
    let g = lerp(fg, tg, t).round() as u8;
    let b = lerp(fb, tb, t).round() as u8;
    let a = lerp(fa, ta, t);
    Some(format!("rgba({},{},{},{:.4})", r, g, b, a))
}

fn parse_rgba(s: &str) -> Option<(f32, f32, f32, f32)> {
    let s = s.trim();
    let inner = s.strip_prefix("rgba(")?.strip_suffix(')')?;
    let mut it = inner.split(',');
    let r = it.next()?.trim().parse::<f32>().ok()?;
    let g = it.next()?.trim().parse::<f32>().ok()?;
    let b = it.next()?.trim().parse::<f32>().ok()?;
    let a = it.next()?.trim().parse::<f32>().ok()?;
    Some((r, g, b, a))
}

/// Interpolate by extracting all decimal numbers from both strings and lerping them.
fn interpolate_numeric(from: &str, to: &str, t: f32) -> String {
    let from_nums = extract_nums(from);
    let to_nums   = extract_nums(to);

    if from_nums.is_empty() || from_nums.len() != to_nums.len() {
        return if t < 0.5 { from.to_string() } else { to.to_string() };
    }

    let mut result = from.to_string();
    // Replace in reverse order so byte offsets remain valid.
    for ((start, end, fv), (_, _, tv)) in from_nums.iter().zip(to_nums.iter()).rev() {
        let v = lerp(*fv, *tv, t);
        let s = if v == v.floor() && v.abs() < 1e9 {
            format!("{}", v as i64)
        } else {
            format!("{:.4}", v)
        };
        result.replace_range(start..end, &s);
    }
    result
}

/// Extract `(start_byte, end_byte, value)` for every number in `s`.
fn extract_nums(s: &str) -> Vec<(usize, usize, f32)> {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let neg = bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit();
        if bytes[i].is_ascii_digit() || neg {
            let start = i;
            if neg { i += 1; }
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') { i += 1; }
            if let Ok(v) = s[start..i].parse::<f32>() {
                result.push((start, i, v));
            }
        } else {
            i += 1;
        }
    }
    result
}

#[inline] fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

/// Apply an easing function to a linear progress value in 0.0..=1.0.
pub(crate) fn apply_easing(easing: &EasingFn, t: f32) -> f32 {
    match easing {
        EasingFn::Linear     => t,
        EasingFn::Ease       => cubic_bezier(0.25, 0.1, 0.25, 1.0, t),
        EasingFn::EaseIn     => cubic_bezier(0.42, 0.0, 1.0,  1.0, t),
        EasingFn::EaseOut    => cubic_bezier(0.0,  0.0, 0.58, 1.0, t),
        EasingFn::EaseInOut  => cubic_bezier(0.42, 0.0, 0.58, 1.0, t),
        EasingFn::CubicBezier(x1, y1, x2, y2) => cubic_bezier(*x1, *y1, *x2, *y2, t),
        EasingFn::StepStart  => if t <= 0.0 { 0.0 } else { 1.0 },
        EasingFn::StepEnd    => if t < 1.0 { 0.0 } else { 1.0 },
        EasingFn::Steps(n, jump_start) => {
            let n = *n as f32;
            if *jump_start { ((t * n).ceil() / n).min(1.0) }
            else           { ((t * n).floor() / n).min(1.0) }
        }
    }
}

/// CSS cubic-bezier evaluation via Newton-Raphson.
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let mut u = t;
    for _ in 0..8 {
        let bx = bcoord(x1, x2, u) - t;
        let db = bderiv(x1, x2, u);
        if db.abs() < 1e-6 { break; }
        u = (u - bx / db).clamp(0.0, 1.0);
    }
    bcoord(y1, y2, u)
}

fn bcoord(p1: f32, p2: f32, t: f32) -> f32 {
    let t2 = t * t; let t3 = t2 * t;
    3.0 * (1.0 - t) * (1.0 - t) * t * p1
        + 3.0 * (1.0 - t) * t2 * p2
        + t3
}

fn bderiv(p1: f32, p2: f32, t: f32) -> f32 {
    3.0 * (1.0 - t) * (1.0 - t) * p1
        + 6.0 * (1.0 - t) * t * (p2 - p1)
        + 3.0 * t * t * (1.0 - p2)
}

impl Document {
    /// Dispatch a DOM event from inside the engine.
    ///
    /// Moves the listener map out so handlers can be given `&mut Document`,
    /// merges back anything they registered, and sweeps `once` listeners.
    /// Every engine dispatch goes through here so none of them can forget a
    /// step.
    pub fn dispatch_dom_event(&mut self, event: &mut crate::dom::events::DomEvent) -> bool {
        if event.target == 0 { return false; }
        // DOM §2.9's dispatch flag: an event already in flight cannot be
        // dispatched again. Without the guard a handler that re-dispatches its
        // own event recurses until the stack runs out.
        if event.is_dispatching() { return false; }
        // `Window` is an EventTarget but not a node, so it has no tree path —
        // the event fires on it directly.
        if crate::dom::events::is_window_target(event.target) {
            let map = std::mem::take(&mut self.event_targets);
            let handled = map.dispatch_path(event, &[crate::dom::events::WINDOW_TARGET], self);
            let added = std::mem::replace(&mut self.event_targets, map);
            self.event_targets.merge_from(added);
            self.event_targets.sweep_removed();
            return handled;
        }
        let map = std::mem::take(&mut self.event_targets);
        let root = std::mem::replace(&mut self.root, WebCore::new("#placeholder"));
        let handled = map.dispatch_on_tree(&root, event, self);
        self.root = root;
        let added = std::mem::replace(&mut self.event_targets, map);
        self.event_targets.merge_from(added);
        self.event_targets.sweep_removed();
        handled
    }
}

impl Document {
    /// The `Window` event target — `window.addEventListener(...)`.
    pub fn window_target(&self) -> u32 { crate::dom::events::WINDOW_TARGET }

    /// Fire a window-level event: `load`, `resize`, `scroll`, `popstate`,
    /// `hashchange`, `beforeunload` and the rest of `WindowEventHandlers`.
    ///
    /// Returns false if a handler cancelled it, which is what `beforeunload`
    /// and `unload` are asked for.
    pub fn fire_window_event(&mut self, event_type: &str) -> bool {
        let mut e = crate::dom::events::DomEvent::new(
            event_type, crate::dom::events::WINDOW_TARGET);
        self.dispatch_dom_event(&mut e);
        !e.default_prevented()
    }
}

impl Document {
    /// Dispatch an engine input event to BOTH listener systems.
    ///
    /// `HtmlEvent` is the engine's internal input record and `DomEvent` is the
    /// WHATWG one; every input has to reach both or half the listeners on a
    /// page never run. Only `click`, `mouseover`, `mouseout` and the keyboard
    /// types were bridged, so `addEventListener("input")`, `"change"`,
    /// `"submit"`, `"focus"` and `"blur"` — among the most-used handlers on the
    /// web — were registered, stored, and never called.
    ///
    /// Returns whether anything handled it. A DOM listener that cancels the
    /// event cancels the default action too.
    pub fn dispatch_input_event(&mut self, mut evt: crate::dom::HtmlEvent) -> (bool, crate::dom::HtmlEvent) {
        let handled = false;
        if evt.target == 0 { return (handled, evt); }
        let mut dom = crate::dom::events::DomEvent::new(evt.event_type.as_str(), evt.target);
        dom.client_x       = evt.client_pos.0;
        dom.client_y       = evt.client_pos.1;
        dom.button         = evt.button;
        dom.key_code       = evt.key_code;
        dom.char_code      = evt.char_code;
        dom.ctrl_key       = evt.ctrl_key;
        dom.shift_key      = evt.shift_key;
        dom.alt_key        = evt.alt_key;
        dom.meta_key       = evt.meta_key;
        dom.delta_x        = evt.delta_x;
        dom.delta_y        = evt.delta_y;
        dom.related_target = evt.related_target;
        dom.key = match evt.char_code {
            Some(c) => c.to_string(),
            None => crate::dom::events::key_name_for_code(evt.key_code).to_string(),
        };
        dom.set_scroll_offset(self.scroll_x, self.scroll_y);
        let dom_handled = self.dispatch_dom_event(&mut dom);
        if dom.default_prevented() { evt.default_prevented = true; }
        (handled || dom_handled, evt)
    }
}
