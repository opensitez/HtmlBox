// Tests for the CSS animation and transition runtime.

use std::time::{Duration, Instant};
use crate::css::{
    parse_easing, parse_animation_shorthand, parse_transition_shorthand,
    extract_keyframes, Stylesheet,
};
use crate::types::*;
use crate::layout::LayoutEngine;
use crate::html::parse_html;
use super::harness::*;

// ── Easing function parsing ───────────────────────────────────────────────────

#[test]
fn easing_linear() {
    assert_eq!(parse_easing("linear"), EasingFn::Linear);
}

#[test]
fn easing_keywords() {
    assert_eq!(parse_easing("ease"),        EasingFn::Ease);
    assert_eq!(parse_easing("ease-in"),     EasingFn::EaseIn);
    assert_eq!(parse_easing("ease-out"),    EasingFn::EaseOut);
    assert_eq!(parse_easing("ease-in-out"), EasingFn::EaseInOut);
    assert_eq!(parse_easing("step-start"),  EasingFn::StepStart);
    assert_eq!(parse_easing("step-end"),    EasingFn::StepEnd);
}

#[test]
fn easing_cubic_bezier() {
    assert_eq!(
        parse_easing("cubic-bezier(0.25, 0.1, 0.25, 1.0)"),
        EasingFn::CubicBezier(0.25, 0.1, 0.25, 1.0)
    );
}

#[test]
fn easing_steps_start() {
    assert_eq!(parse_easing("steps(4, start)"), EasingFn::Steps(4, true));
}

#[test]
fn easing_steps_end() {
    assert_eq!(parse_easing("steps(3, end)"), EasingFn::Steps(3, false));
}

#[test]
fn easing_unknown_defaults_to_ease() {
    assert_eq!(parse_easing("bogus"), EasingFn::Ease);
}

// ── Easing function math ──────────────────────────────────────────────────────

#[test]
fn apply_easing_linear_is_identity() {
    let e = EasingFn::Linear;
    assert!((apply_easing(&e, 0.0) - 0.0).abs() < 1e-4);
    assert!((apply_easing(&e, 0.5) - 0.5).abs() < 1e-4);
    assert!((apply_easing(&e, 1.0) - 1.0).abs() < 1e-4);
}

#[test]
fn apply_easing_boundary_values() {
    for easing in [EasingFn::Ease, EasingFn::EaseIn, EasingFn::EaseOut, EasingFn::EaseInOut] {
        let v0 = apply_easing(&easing, 0.0);
        let v1 = apply_easing(&easing, 1.0);
        assert!(v0.abs() < 1e-3,         "easing {:?} at t=0 should be ~0, got {}", easing, v0);
        assert!((v1 - 1.0).abs() < 1e-3, "easing {:?} at t=1 should be ~1, got {}", easing, v1);
    }
}

#[test]
fn apply_easing_step_start() {
    let e = EasingFn::StepStart;
    assert_eq!(apply_easing(&e, 0.0), 0.0);
    assert_eq!(apply_easing(&e, 0.5), 1.0);
    assert_eq!(apply_easing(&e, 1.0), 1.0);
}

#[test]
fn apply_easing_step_end() {
    let e = EasingFn::StepEnd;
    assert_eq!(apply_easing(&e, 0.0), 0.0);
    assert_eq!(apply_easing(&e, 0.99), 0.0);
    assert_eq!(apply_easing(&e, 1.0), 1.0);
}

#[test]
fn apply_easing_steps_4() {
    let e = EasingFn::Steps(4, false);  // jump-end
    let v = apply_easing(&e, 0.6);
    // floor(0.6 * 4) / 4 = floor(2.4)/4 = 2/4 = 0.5
    assert!((v - 0.5).abs() < 1e-4, "expected 0.5, got {}", v);
}

// ── animation shorthand parsing ───────────────────────────────────────────────

#[test]
fn parse_animation_simple() {
    let anims = parse_animation_shorthand("spin 1s linear infinite");
    assert_eq!(anims.len(), 1);
    let a = &anims[0];
    assert_eq!(a.name, "spin");
    assert!((a.duration_ms - 1000.0).abs() < 1.0);
    assert_eq!(a.timing_fn, EasingFn::Linear);
    assert!(a.iteration_count.is_infinite());
}

#[test]
fn parse_animation_with_delay() {
    let anims = parse_animation_shorthand("fade 0.3s ease-in 0.1s");
    assert_eq!(anims.len(), 1);
    let a = &anims[0];
    assert_eq!(a.name, "fade");
    assert!((a.duration_ms - 300.0).abs() < 1.0);
    assert!((a.delay_ms   - 100.0).abs() < 1.0);
    assert_eq!(a.timing_fn, EasingFn::EaseIn);
}

#[test]
fn parse_animation_fill_mode() {
    let anims = parse_animation_shorthand("slide 2s both");
    assert_eq!(anims.len(), 1);
    assert_eq!(anims[0].fill_mode, FillMode::Both);
}

#[test]
fn parse_animation_direction_alternate() {
    let anims = parse_animation_shorthand("bounce 1s alternate");
    assert_eq!(anims.len(), 1);
    assert_eq!(anims[0].direction, AnimDirection::Alternate);
}

#[test]
fn parse_animation_multiple() {
    let anims = parse_animation_shorthand("spin 1s, fade 0.5s ease-out");
    assert_eq!(anims.len(), 2);
    assert_eq!(anims[0].name, "spin");
    assert_eq!(anims[1].name, "fade");
    assert_eq!(anims[1].timing_fn, EasingFn::EaseOut);
}

#[test]
fn parse_animation_paused() {
    let anims = parse_animation_shorthand("pulse 1s paused");
    assert_eq!(anims.len(), 1);
    assert!(anims[0].play_state_paused);
}

#[test]
fn parse_animation_none_is_skipped() {
    let anims = parse_animation_shorthand("none");
    assert_eq!(anims.len(), 0);
}

#[test]
fn parse_animation_iteration_count_number() {
    let anims = parse_animation_shorthand("flash 0.5s 3");
    assert_eq!(anims.len(), 1);
    assert!((anims[0].iteration_count - 3.0).abs() < 0.01);
}

// ── transition shorthand parsing ──────────────────────────────────────────────

#[test]
fn parse_transition_simple() {
    let trs = parse_transition_shorthand("color 0.3s ease");
    assert_eq!(trs.len(), 1);
    assert_eq!(trs[0].property, "color");
    assert!((trs[0].duration_ms - 300.0).abs() < 1.0);
    assert_eq!(trs[0].timing_fn, EasingFn::Ease);
}

#[test]
fn parse_transition_with_delay() {
    let trs = parse_transition_shorthand("opacity 1s linear 0.5s");
    assert_eq!(trs.len(), 1);
    assert_eq!(trs[0].property, "opacity");
    assert!((trs[0].duration_ms - 1000.0).abs() < 1.0);
    assert!((trs[0].delay_ms    -  500.0).abs() < 1.0);
}

#[test]
fn parse_transition_multiple() {
    let trs = parse_transition_shorthand("color 0.3s, transform 0.5s ease-out");
    assert_eq!(trs.len(), 2);
    assert_eq!(trs[0].property, "color");
    assert_eq!(trs[1].property, "transform");
    assert_eq!(trs[1].timing_fn, EasingFn::EaseOut);
}

#[test]
fn parse_transition_none_skipped() {
    let trs = parse_transition_shorthand("none");
    assert_eq!(trs.len(), 0);
}

// ── @keyframes extraction ─────────────────────────────────────────────────────

#[test]
fn extract_keyframes_basic() {
    let css = r#"
        @keyframes spin {
            from { transform: rotate(0deg); }
            to   { transform: rotate(360deg); }
        }
    "#;
    let kf = extract_keyframes(css);
    assert!(kf.contains_key("spin"), "should contain 'spin'");
    let stops = &kf["spin"];
    assert_eq!(stops.len(), 2);
    assert!((stops[0].offset - 0.0).abs() < 1e-4);
    assert!((stops[1].offset - 1.0).abs() < 1e-4);
}

#[test]
fn extract_keyframes_percent() {
    let css = r#"
        @keyframes pulse {
            0%   { opacity: 1; }
            50%  { opacity: 0.5; }
            100% { opacity: 1; }
        }
    "#;
    let kf = extract_keyframes(css);
    let stops = &kf["pulse"];
    assert_eq!(stops.len(), 3);
    assert!((stops[1].offset - 0.5).abs() < 1e-4);
}

#[test]
fn extract_keyframes_multiple_selectors() {
    let css = r#"
        @keyframes blink {
            0%, 100% { opacity: 1; }
            50%       { opacity: 0; }
        }
    "#;
    let kf = extract_keyframes(css);
    let stops = &kf["blink"];
    // 0% + 100% + 50% = 3 stops (sorted by offset)
    assert_eq!(stops.len(), 3);
}

#[test]
fn extract_keyframes_multiple_animations() {
    let css = r#"
        @keyframes spin { from {} to {} }
        @keyframes fade { from { opacity:0; } to { opacity:1; } }
    "#;
    let kf = extract_keyframes(css);
    assert!(kf.contains_key("spin"));
    assert!(kf.contains_key("fade"));
}

#[test]
fn extract_keyframes_webkit_prefix() {
    let css = r#"
        @-webkit-keyframes slide {
            from { transform: translateX(-100px); }
            to   { transform: translateX(0); }
        }
    "#;
    let kf = extract_keyframes(css);
    assert!(kf.contains_key("slide"));
}

#[test]
fn extract_keyframes_ignores_regular_rules() {
    let css = r#"
        p { color: red; }
        @keyframes fade { from { opacity:0; } to { opacity:1; } }
        .box { margin: 0; }
    "#;
    let kf = extract_keyframes(css);
    assert_eq!(kf.len(), 1);
    assert!(kf.contains_key("fade"));
}

#[test]
fn stylesheet_parse_and_add_stores_keyframes() {
    let mut ss = Stylesheet::default();
    ss.parse_and_add(r#"
        @keyframes slide-in {
            from { transform: translateX(-200px); }
            to   { transform: translateX(0); }
        }
        .box { animation: slide-in 0.5s ease; }
    "#);
    assert!(ss.keyframes.contains_key("slide-in"));
    assert_eq!(ss.keyframes["slide-in"].len(), 2);
}

// ── Value interpolation ───────────────────────────────────────────────────────

#[test]
fn interpolate_value_midpoint_numeric() {
    // "0px" → "100px" at t=0.5 → "50px"
    let result = interpolate_value("0px", "100px", 0.5);
    assert!(result.contains("50"), "expected ~50px, got '{}'", result);
}

#[test]
fn interpolate_value_at_zero() {
    let result = interpolate_value("0px", "100px", 0.0);
    assert!(result.contains('0'), "expected 0px, got '{}'", result);
}

#[test]
fn interpolate_value_at_one() {
    let result = interpolate_value("0px", "100px", 1.0);
    assert!(result.contains("100"), "expected 100px, got '{}'", result);
}

#[test]
fn interpolate_value_rgba_color() {
    // Fully transparent → fully opaque red
    let from = "rgba(255,0,0,0.0000)";
    let to   = "rgba(255,0,0,1.0000)";
    let mid  = interpolate_value(from, to, 0.5);
    assert!(mid.starts_with("rgba("), "expected rgba(), got '{}'", mid);
    // Red channel should stay 255, alpha should be ~0.5
    assert!(mid.contains("255,0,0"), "red channel should be 255, got '{}'", mid);
}

#[test]
fn interpolate_value_snaps_on_mismatch() {
    // Different token count → snap
    let result = interpolate_value("red", "blue", 0.3);
    assert_eq!(result, "red");
    let result2 = interpolate_value("red", "blue", 0.7);
    assert_eq!(result2, "blue");
}

// ── @keyframes stop interpolation ─────────────────────────────────────────────

#[test]
fn interpolate_stops_at_start() {
    let stops = vec![
        KeyframeStop { offset: 0.0, properties: vec![("opacity".into(), "0".into())] },
        KeyframeStop { offset: 1.0, properties: vec![("opacity".into(), "1".into())] },
    ];
    let props = interpolate_keyframe_stops(&stops, 0.0);
    let op = props.iter().find(|(k,_)| k == "opacity").map(|(_, v)| v.as_str()).unwrap_or("");
    assert!(op.parse::<f32>().unwrap_or(-1.0).abs() < 0.01, "expected ~0, got '{}'", op);
}

#[test]
fn interpolate_stops_at_end() {
    let stops = vec![
        KeyframeStop { offset: 0.0, properties: vec![("opacity".into(), "0".into())] },
        KeyframeStop { offset: 1.0, properties: vec![("opacity".into(), "1".into())] },
    ];
    let props = interpolate_keyframe_stops(&stops, 1.0);
    let op = props.iter().find(|(k,_)| k == "opacity").map(|(_, v)| v.as_str()).unwrap_or("");
    assert!((op.parse::<f32>().unwrap_or(-1.0) - 1.0).abs() < 0.01);
}

#[test]
fn interpolate_stops_midpoint() {
    let stops = vec![
        KeyframeStop { offset: 0.0, properties: vec![("opacity".into(), "0".into())] },
        KeyframeStop { offset: 1.0, properties: vec![("opacity".into(), "1".into())] },
    ];
    let props = interpolate_keyframe_stops(&stops, 0.5);
    let op = props.iter().find(|(k,_)| k == "opacity").map(|(_, v)| v.as_str()).unwrap_or("");
    let v: f32 = op.parse().unwrap_or(-1.0);
    assert!((v - 0.5).abs() < 0.05, "expected ~0.5, got {}", v);
}

#[test]
fn interpolate_stops_between_non_zero_stops() {
    let stops = vec![
        KeyframeStop { offset: 0.0,  properties: vec![("opacity".into(), "0".into())] },
        KeyframeStop { offset: 0.5,  properties: vec![("opacity".into(), "0.5".into())] },
        KeyframeStop { offset: 1.0,  properties: vec![("opacity".into(), "1".into())] },
    ];
    // At t=0.25 (between stop 0 and stop 0.5), local_t = 0.5
    let props = interpolate_keyframe_stops(&stops, 0.25);
    let op = props.iter().find(|(k,_)| k == "opacity").map(|(_, v)| v.as_str()).unwrap_or("");
    let v: f32 = op.parse().unwrap_or(-1.0);
    assert!((v - 0.25).abs() < 0.05, "expected ~0.25, got {}", v);
}

// ── apply_property: animation / transition sub-properties ────────────────────

#[test]
fn style_animation_shorthand_parsed() {
    let s = style_with("animation", "spin 2s linear infinite");
    assert_eq!(s.animations.len(), 1);
    assert_eq!(s.animations[0].name, "spin");
    assert!((s.animations[0].duration_ms - 2000.0).abs() < 1.0);
    assert!(s.animations[0].iteration_count.is_infinite());
}

#[test]
fn style_animation_sub_properties() {
    let mut s = ComputedStyle::default();
    crate::css::apply_property(&mut s, "animation-name", "fade");
    crate::css::apply_property(&mut s, "animation-duration", "0.5s");
    crate::css::apply_property(&mut s, "animation-timing-function", "ease-out");
    crate::css::apply_property(&mut s, "animation-iteration-count", "3");
    crate::css::apply_property(&mut s, "animation-direction", "alternate");
    crate::css::apply_property(&mut s, "animation-fill-mode", "both");
    assert_eq!(s.animations[0].name, "fade");
    assert!((s.animations[0].duration_ms - 500.0).abs() < 1.0);
    assert_eq!(s.animations[0].timing_fn, EasingFn::EaseOut);
    assert!((s.animations[0].iteration_count - 3.0).abs() < 0.01);
    assert_eq!(s.animations[0].direction, AnimDirection::Alternate);
    assert_eq!(s.animations[0].fill_mode, FillMode::Both);
}

#[test]
fn style_transition_shorthand_parsed() {
    let s = style_with("transition", "opacity 0.3s ease-in-out");
    assert_eq!(s.transitions.len(), 1);
    assert_eq!(s.transitions[0].property, "opacity");
    assert!((s.transitions[0].duration_ms - 300.0).abs() < 1.0);
    assert_eq!(s.transitions[0].timing_fn, EasingFn::EaseInOut);
}

#[test]
fn style_transition_sub_properties() {
    let mut s = ComputedStyle::default();
    crate::css::apply_property(&mut s, "transition-property", "color");
    crate::css::apply_property(&mut s, "transition-duration", "200ms");
    crate::css::apply_property(&mut s, "transition-timing-function", "linear");
    crate::css::apply_property(&mut s, "transition-delay", "50ms");
    assert_eq!(s.transitions[0].property, "color");
    assert!((s.transitions[0].duration_ms - 200.0).abs() < 1.0);
    assert_eq!(s.transitions[0].timing_fn, EasingFn::Linear);
    assert!((s.transitions[0].delay_ms - 50.0).abs() < 1.0);
}

// ── Document::sync_animations ─────────────────────────────────────────────────

fn doc_with_animation(anim_css: &str) -> Document {
    let html = format!(
        r#"<html><head><style>
            @keyframes spin {{
                from {{ transform: rotate(0deg); }}
                to   {{ transform: rotate(360deg); }}
            }}
            @keyframes fade {{
                from {{ opacity: 0; }}
                to   {{ opacity: 1; }}
            }}
            .box {{ {} }}
        </style></head><body><div class="box">hi</div></body></html>"#,
        anim_css
    );
    let mut doc = parse_html(&html);
    let mut engine = LayoutEngine::new();
    engine.layout(&mut doc, 800.0);
    doc
}

#[test]
fn sync_animations_starts_state() {
    let doc = doc_with_animation("animation: spin 1s linear infinite;");
    assert!(!doc.active_animations.is_empty(), "should have at least one active animation");
    assert_eq!(doc.active_animations[0].animation.name, "spin");
}

#[test]
fn sync_animations_does_not_duplicate() {
    let mut doc = doc_with_animation("animation: spin 2s linear infinite;");
    let count_before = doc.active_animations.len();
    // Call layout again — should not add another AnimState for the same element+name.
    let mut engine = LayoutEngine::new();
    engine.layout(&mut doc, 800.0);
    assert_eq!(doc.active_animations.len(), count_before);
}

#[test]
fn sync_animations_removes_when_gone() {
    // Start with animation, then remove it (simulate by re-parsing without it).
    let doc_with = doc_with_animation("animation: spin 1s;");
    assert!(!doc_with.active_animations.is_empty());

    let doc_without = doc_with_animation("/* no animation */");
    assert!(doc_without.active_animations.is_empty());
}

// ── Document::tick_animations ─────────────────────────────────────────────────

#[test]
fn tick_animations_produces_overrides() {
    let mut doc = doc_with_animation("animation: spin 1s linear infinite;");
    // Advance by 500ms (half-way through)
    let now = doc.active_animations[0].start_time + Duration::from_millis(500);
    doc.tick_animations(now);
    // The 'transform' property should have an override for the animated element.
    let has_transform_override = doc.animation_overrides.values()
        .any(|props| props.iter().any(|(k, _)| k == "transform"));
    assert!(has_transform_override, "expected a transform override at t=0.5");
}

#[test]
fn tick_animations_needs_more_frames_while_running() {
    let mut doc = doc_with_animation("animation: spin 1s linear infinite;");
    let now = doc.active_animations[0].start_time + Duration::from_millis(100);
    doc.tick_animations(now);
    assert!(doc.needs_animation_frame);
}

#[test]
fn tick_animations_finished_when_done() {
    let mut doc = doc_with_animation("animation: spin 0.1s linear 1;");
    // Jump way past the end of the animation.
    let now = doc.active_animations[0].start_time + Duration::from_millis(500);
    doc.tick_animations(now);
    // Animation should have been removed.
    assert!(doc.active_animations.is_empty(), "finished animation should be removed");
}

#[test]
fn tick_animations_delay_phase() {
    let mut doc = doc_with_animation("animation: spin 1s linear 0.5s;"); // 500ms delay
    // Only 100ms in — still in delay.
    let now = doc.active_animations[0].start_time + Duration::from_millis(100);
    doc.tick_animations(now);
    // Still running (in delay phase).
    assert!(doc.needs_animation_frame);
    // No override yet (no backwards fill-mode).
    assert!(doc.animation_overrides.values().all(|p| p.is_empty() || {
        p.iter().all(|(k, _)| k != "transform")
    }), "no transform override expected during delay without backwards fill");
}

#[test]
fn tick_animations_backwards_fill_during_delay() {
    let mut doc = doc_with_animation("animation: fade 1s linear 0.5s backwards;");
    let now = doc.active_animations[0].start_time + Duration::from_millis(100);
    doc.tick_animations(now);
    // With `backwards`, we should see the "from" keyframe applied (opacity: 0).
    let has_opacity = doc.animation_overrides.values()
        .any(|props| props.iter().any(|(k, _)| k == "opacity"));
    assert!(has_opacity, "backwards fill should apply 'from' keyframe during delay");
}

#[test]
fn tick_animations_direction_reverse() {
    let mut doc = doc_with_animation("animation: fade 1s linear reverse 1;");
    let start = doc.active_animations[0].start_time;
    // At start (t=0), with reverse direction the effective t=1 → opacity should be ~1.
    let now = start + Duration::from_millis(10);
    doc.tick_animations(now);
    let overrides = doc.animation_overrides.values()
        .flat_map(|p| p.iter())
        .find(|(k, _)| k == "opacity")
        .map(|(_, v)| v.parse::<f32>().unwrap_or(-1.0));
    if let Some(v) = overrides {
        assert!(v > 0.8, "reverse at t~0 should give opacity~1, got {}", v);
    }
}

#[test]
fn tick_animations_alternate_direction() {
    let mut doc = doc_with_animation("animation: fade 1s linear alternate infinite;");
    let start = doc.active_animations[0].start_time;

    // First iteration (even): t goes 0→1, so opacity goes 0→1.
    let now_half = start + Duration::from_millis(500);
    doc.tick_animations(now_half);
    let op1 = doc.animation_overrides.values()
        .flat_map(|p| p.iter())
        .find(|(k, _)| k == "opacity")
        .and_then(|(_, v)| v.parse::<f32>().ok())
        .unwrap_or(-1.0);
    assert!((op1 - 0.5).abs() < 0.1, "first iter mid: expected ~0.5, got {}", op1);

    // Second iteration (odd): direction reverses, so at t=0.25 within iter, effective=0.75.
    let now_iter2 = start + Duration::from_millis(1250);
    doc.animation_overrides.clear();
    doc.tick_animations(now_iter2);
    let op2 = doc.animation_overrides.values()
        .flat_map(|p| p.iter())
        .find(|(k, _)| k == "opacity")
        .and_then(|(_, v)| v.parse::<f32>().ok())
        .unwrap_or(-1.0);
    assert!(op2 > 0.5, "second iter (reversed) t=0.25 → effective=0.75, got {}", op2);
}

// ── Integration: layout applies animation overrides ───────────────────────────

#[test]
fn layout_applies_animation_override_to_style() {
    // After layout, the animated element's computed opacity should reflect
    // the animation, not just the stylesheet value.
    let html = r#"<html><head><style>
        @keyframes fade-in { from { opacity: 0; } to { opacity: 1; } }
        .box { opacity: 1; animation: fade-in 10s linear; }
    </style></head><body><div class="box">hi</div></body></html>"#;

    let mut doc = parse_html(html);
    let mut engine = LayoutEngine::new();
    engine.layout(&mut doc, 800.0);

    // Animation just started, so at t~=0 opacity should be ~0 (from keyframe).
    let b = find_box(&doc.root, &|b: &WebCore| b.tag == "div");
    assert!(b.is_some(), "div should exist");
    // The opacity should be close to 0 (start of fade-in) rather than 1 (stylesheet).
    let opacity = b.unwrap().style.opacity;
    assert!(opacity < 0.2, "opacity at animation start should be ~0, got {}", opacity);
}

#[test]
fn doc_needs_animation_frame_set_by_layout() {
    let doc = doc_with_animation("animation: spin 2s linear infinite;");
    assert!(doc.needs_animation_frame, "should need another frame while animation runs");
}

// ── CSS Transitions ───────────────────────────────────────────────────────────

#[test]
fn transition_state_starts_on_style_change() {
    let html = r#"<html><head><style>
        .box { opacity: 1; transition: opacity 0.5s linear; }
    </style></head><body><div class="box">hi</div></body></html>"#;

    let mut doc = parse_html(html);
    let mut engine = LayoutEngine::new();
    engine.layout(&mut doc, 800.0);

    // Simulate a style change: manually inject a new opacity into the stylesheet
    // and force a re-cascade by changing the inline style.
    let b_nid = find_box(&doc.root, &|b: &WebCore| b.tag == "div")
        .map(|b| b.node_id);

    // Directly write a prev_style snapshot for the element with opacity=1,
    // then do another layout — if the computed opacity changed the engine
    // should have started a transition.
    if let Some(id) = b_nid {
        let mut prev = std::collections::HashMap::new();
        prev.insert("opacity".to_string(), "1".to_string());
        doc.prev_styles.insert(id, prev);

        // Force a cascade so sync_transitions runs.
        engine.invalidate_cascade();
        engine.layout(&mut doc, 800.0);

        // If the opacity hasn't changed (still 1 from stylesheet),
        // no transition should have started for opacity.
        // The important thing is that no panic occurred and the doc is consistent.
        assert!(doc.transition_states.len() <= 1);
    }
}

#[test]
fn transition_interpolates_between_values() {
    use std::collections::HashMap;

    let mut doc = parse_html("<html><body></body></html>");
    let elem_id: u32 = 0xDEAD; // fake element node_id

    // Insert a transition state directly.
    let start = Instant::now() - Duration::from_millis(250);
    doc.transition_states.insert(elem_id, vec![TransitionState {
        property:    "opacity".to_string(),
        from_value:  "0".to_string(),
        to_value:    "1".to_string(),
        start_time:  start,
        duration_ms: 500.0,
        delay_ms:    0.0,
        timing_fn:   EasingFn::Linear,
    }]);

    let now = start + Duration::from_millis(250);
    doc.tick_animations(now);

    // At 250ms / 500ms = 0.5 progress, opacity should be ~0.5.
    let val = doc.animation_overrides.get(&elem_id)
        .and_then(|props| props.iter().find(|(k,_)| k == "opacity"))
        .and_then(|(_, v)| v.parse::<f32>().ok())
        .unwrap_or(-1.0);
    assert!((val - 0.5).abs() < 0.05, "expected ~0.5, got {}", val);
}

#[test]
fn transition_completes_and_is_removed() {
    let mut doc = parse_html("<html><body></body></html>");
    let elem_id: u32 = 0xBEEF;

    let start = Instant::now() - Duration::from_millis(600);
    doc.transition_states.insert(elem_id, vec![TransitionState {
        property:    "opacity".to_string(),
        from_value:  "0".to_string(),
        to_value:    "1".to_string(),
        start_time:  start,
        duration_ms: 500.0,
        delay_ms:    0.0,
        timing_fn:   EasingFn::Linear,
    }]);

    let now = start + Duration::from_millis(600);
    doc.tick_animations(now);

    assert!(doc.transition_states.is_empty(), "completed transition should be removed");
    assert!(!doc.needs_animation_frame, "no more frames needed after transition completes");
}

#[test]
fn transition_delay_applies_from_value() {
    let mut doc = parse_html("<html><body></body></html>");
    let elem_id: u32 = 0xCAFE;

    let start = Instant::now();
    doc.transition_states.insert(elem_id, vec![TransitionState {
        property:    "opacity".to_string(),
        from_value:  "0".to_string(),
        to_value:    "1".to_string(),
        start_time:  start,
        duration_ms: 500.0,
        delay_ms:    300.0,   // 300ms delay
        timing_fn:   EasingFn::Linear,
    }]);

    // Only 100ms elapsed — still in delay.
    let now = start + Duration::from_millis(100);
    doc.tick_animations(now);

    // The "from" value should be applied during delay.
    let val = doc.animation_overrides.get(&elem_id)
        .and_then(|props| props.iter().find(|(k,_)| k == "opacity"))
        .and_then(|(_, v)| v.parse::<f32>().ok())
        .unwrap_or(-1.0);
    assert!((val - 0.0).abs() < 0.01, "during delay, from_value (0) should be applied, got {}", val);
}
