//! Driving CSS animations and transitions over time.

#![allow(unused_imports)]
use super::*;
use crate::css::*;
use crate::dom::*;
use crate::html::*;
use crate::layout::LayoutEngine;
use std::collections::{HashMap, HashSet};

impl Document {
    /// Walk the tree and ensure an `AnimState` exists for every element that
    /// currently has an `animation` property.  Call this after each cascade pass.
    pub fn sync_animations(&mut self, now: std::time::Instant) {
        let mut current: Vec<(u32, ParsedAnimation)> = Vec::new();
        fn collect(node: &WebCore, out: &mut Vec<(u32, ParsedAnimation)>) {
            let id = node.node_id;
            for a in &node.style.rare().animations {
                out.push((id, a.clone()));
            }
            for child in &node.children {
                collect(child, out);
            }
        }
        collect(&self.root, &mut current);

        // Start animations that aren't tracked yet.
        for (id, anim) in &current {
            let running = self
                .active_animations
                .iter()
                .any(|s| s.element_id == *id && s.animation.name == anim.name);
            if !running && !anim.name.is_empty() && anim.name != "none" {
                self.active_animations.push(AnimState {
                    element_id: *id,
                    animation: anim.clone(),
                    start_time: now,
                });
            }
        }

        // Remove animations whose element no longer carries that animation name.
        self.active_animations.retain(|s| {
            current
                .iter()
                .any(|(id, a)| *id == s.element_id && a.name == s.animation.name)
        });
    }

    /// Detect CSS property changes caused by the cascade and start transitions.
    /// `cascade_ran`: true when the full cascade just ran (node.style is clean).
    /// When false (hover-only change), base values are read from `cascade_styles`
    /// so animation-overridden node.style values don't pollute change detection.
    pub fn sync_transitions(&mut self, now: std::time::Instant, cascade_ran: bool) {
        let hovered = self.hovered_box;
        let mut current: Vec<(u32, Vec<ParsedTransition>, HashMap<String, String>)> = Vec::new();
        fn collect(
            node: &WebCore,
            hovered: u32,
            cascade_ran: bool,
            cascade_styles: &HashMap<u32, HashMap<String, String>>,
            out: &mut Vec<(u32, Vec<ParsedTransition>, HashMap<String, String>)>,
        ) {
            let id = node.node_id;
            if !node.style.rare().transitions.is_empty() {
                // Base values: use the clean cascade snapshot when available, so that
                // animation_overrides applied to node.style don't corrupt detection.
                let base = if cascade_ran {
                    extract_transitionable(node)
                } else {
                    cascade_styles
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| extract_transitionable(node))
                };
                let mut vals = base.clone();
                // When hovered, overlay hover_style to get the "target" state.
                if hovered != 0 && subtree_contains_id(node, hovered) {
                    if let Some(hs) = &node.style.hover_style {
                        let hover_vals = extract_transitionable_style(hs);
                        for (k, v) in hover_vals {
                            vals.insert(k, v);
                        }
                    }
                }
                out.push((id, node.style.rare().transitions.clone(), vals));
            }
            for child in &node.children {
                collect(child, hovered, cascade_ran, cascade_styles, out);
            }
        }
        collect(
            &self.root,
            hovered,
            cascade_ran,
            &self.cascade_styles,
            &mut current,
        );

        // When cascade ran, save the clean base styles for hover-only frames.
        if cascade_ran {
            fn snapshot(node: &WebCore, out: &mut HashMap<u32, HashMap<String, String>>) {
                if !node.style.rare().transitions.is_empty() {
                    out.insert(node.node_id, extract_transitionable(node));
                }
                for child in &node.children {
                    snapshot(child, out);
                }
            }
            snapshot(&self.root, &mut self.cascade_styles);
        }

        for (elem_id, trs, cur_vals) in &current {
            let prev = self.prev_styles.get(elem_id).cloned().unwrap_or_default();

            for tr in trs {
                if tr.duration_ms <= 0.0 {
                    continue;
                }
                let props: Vec<&str> = if tr.property == "all" {
                    cur_vals.keys().map(|s| s.as_str()).collect()
                } else {
                    vec![tr.property.as_str()]
                };

                for prop in props {
                    let cur = match cur_vals.get(prop) {
                        Some(v) => v.as_str(),
                        None => continue,
                    };
                    let prv = match prev.get(prop) {
                        Some(v) => v.as_str(),
                        None => {
                            continue;
                        }
                    };
                    if prv == cur {
                        // Uncomment to debug: eprintln!("[TR-SKIP] {} same={:?}", prop, cur);
                        continue;
                    }

                    // Already transitioning to this value?
                    let already = self
                        .transition_states
                        .entry(*elem_id)
                        .or_default()
                        .iter()
                        .any(|t| t.property == prop && t.to_value == cur);
                    if already {
                        continue;
                    }

                    // If a transition is already running for this property, start the
                    // new one from the current animated value (not from prev_styles) to
                    // avoid a visual jump to the original from/to endpoint.
                    let from_val = self
                        .animation_overrides
                        .get(elem_id)
                        .and_then(|ov| ov.iter().find(|(p, _)| p == prop))
                        .map(|(_, v)| v.as_str())
                        .unwrap_or(prv);
                    let entry = self.transition_states.entry(*elem_id).or_default();
                    entry.retain(|t| t.property != prop);
                    entry.push(TransitionState {
                        property: prop.to_string(),
                        from_value: from_val.to_string(),
                        to_value: cur.to_string(),
                        start_time: now,
                        duration_ms: tr.duration_ms,
                        delay_ms: tr.delay_ms,
                        timing_fn: tr.timing_fn.clone(),
                    });
                }
            }
            self.prev_styles.insert(*elem_id, cur_vals.clone());
        }
    }

    /// Advance all running animations and transitions to time `now`.
    /// Populates `animation_overrides` with interpolated CSS values.
    /// Sets `needs_animation_frame = true` if any animation/transition is still running.
    pub fn tick_animations(&mut self, now: std::time::Instant) {
        self.animation_overrides.clear();
        let keyframes = self.stylesheet.keyframes.clone();
        let mut still_running = false;

        // ── CSS Animations ───────────────────────────────────────────────────
        let mut done: Vec<usize> = Vec::new();
        for (idx, state) in self.active_animations.iter().enumerate() {
            let elapsed_ms = now.duration_since(state.start_time).as_secs_f32() * 1000.0;
            let delayed_ms = elapsed_ms - state.animation.delay_ms;

            if delayed_ms < 0.0 {
                // Delay phase: apply backwards fill if needed.
                if matches!(
                    state.animation.fill_mode,
                    FillMode::Backwards | FillMode::Both
                ) {
                    if let Some(kf) = keyframes.get(&state.animation.name) {
                        if let Some(first) = kf.first() {
                            let entry = self
                                .animation_overrides
                                .entry(state.element_id)
                                .or_default();
                            entry.extend(first.properties.clone());
                        }
                    }
                }
                still_running = true;
                continue;
            }

            let duration = state.animation.duration_ms;
            if duration <= 0.0 {
                done.push(idx);
                continue;
            }

            let total_progress = delayed_ms / duration;
            let iteration = total_progress.floor();
            let t_frac = total_progress.fract();
            let iteration_count = state.animation.iteration_count;

            if !iteration_count.is_infinite() && delayed_ms >= duration * iteration_count {
                // Finished: apply forwards fill if needed.
                if matches!(
                    state.animation.fill_mode,
                    FillMode::Forwards | FillMode::Both
                ) {
                    if let Some(kf) = keyframes.get(&state.animation.name) {
                        let endpoint_frac = iteration_count.fract();
                        let final_iteration = if endpoint_frac == 0.0 {
                            (iteration_count - 1.0).max(0.0).floor()
                        } else {
                            iteration_count.floor()
                        };
                        let base_t = if endpoint_frac == 0.0 {
                            1.0
                        } else {
                            endpoint_frac
                        };
                        let final_t = match state.animation.direction {
                            AnimDirection::Normal => base_t,
                            AnimDirection::Reverse => 1.0 - base_t,
                            AnimDirection::Alternate => {
                                if (final_iteration as u32) % 2 == 0 {
                                    base_t
                                } else {
                                    1.0 - base_t
                                }
                            }
                            AnimDirection::AlternateReverse => {
                                if (final_iteration as u32) % 2 == 0 {
                                    1.0 - base_t
                                } else {
                                    base_t
                                }
                            }
                        };
                        let props = interpolate_keyframe_stops(kf, final_t);
                        let entry = self
                            .animation_overrides
                            .entry(state.element_id)
                            .or_default();
                        entry.extend(props);
                    }
                }
                done.push(idx);
                continue;
            }
            still_running = true;

            let effective_t = match state.animation.direction {
                AnimDirection::Normal => t_frac,
                AnimDirection::Reverse => 1.0 - t_frac,
                AnimDirection::Alternate => {
                    if (iteration as u32) % 2 == 0 {
                        t_frac
                    } else {
                        1.0 - t_frac
                    }
                }
                AnimDirection::AlternateReverse => {
                    if (iteration as u32) % 2 == 0 {
                        1.0 - t_frac
                    } else {
                        t_frac
                    }
                }
            };
            let eased = apply_easing(&state.animation.timing_fn, effective_t);

            if let Some(kf) = keyframes.get(&state.animation.name) {
                let props = interpolate_keyframe_stops(kf, eased);
                let entry = self
                    .animation_overrides
                    .entry(state.element_id)
                    .or_default();
                entry.extend(props);
            }
        }
        for idx in done.into_iter().rev() {
            self.active_animations.remove(idx);
        }

        // ── CSS Transitions ──────────────────────────────────────────────────
        let mut empty_elems: Vec<u32> = Vec::new();
        for (elem_id, trs) in &mut self.transition_states {
            let mut done_trs: Vec<usize> = Vec::new();
            for (i, tr) in trs.iter().enumerate() {
                let elapsed_ms = now.duration_since(tr.start_time).as_secs_f32() * 1000.0;
                let delayed_ms = elapsed_ms - tr.delay_ms;

                if delayed_ms < 0.0 {
                    // Apply "from" value during delay.
                    let entry = self.animation_overrides.entry(*elem_id).or_default();
                    entry.push((tr.property.clone(), tr.from_value.clone()));
                    still_running = true;
                    continue;
                }
                if tr.duration_ms <= 0.0 {
                    done_trs.push(i);
                    continue;
                }

                let progress = (delayed_ms / tr.duration_ms).min(1.0);
                if progress >= 1.0 {
                    // Write the final value into animation_overrides so that
                    // transitioning_ids still contains this element for the
                    // completion frame.  Without this, has_transition becomes
                    // false while is_hovered may still be true, causing the
                    // renderer to pick hover_style's color instead of the
                    // correctly-reverted base color.
                    let entry = self.animation_overrides.entry(*elem_id).or_default();
                    entry.push((tr.property.clone(), tr.to_value.clone()));
                    done_trs.push(i);
                    continue;
                }

                still_running = true;
                let eased = apply_easing(&tr.timing_fn, progress);
                let interp =
                    interpolate_property_value(&tr.property, &tr.from_value, &tr.to_value, eased);
                let entry = self.animation_overrides.entry(*elem_id).or_default();
                entry.push((tr.property.clone(), interp));
            }
            for idx in done_trs.into_iter().rev() {
                trs.remove(idx);
            }
            if trs.is_empty() {
                empty_elems.push(*elem_id);
            }
        }
        for eid in empty_elems {
            self.transition_states.remove(&eid);
        }

        self.needs_animation_frame = still_running;

        // Mark all elements with active overrides as layout_dirty so the
        // layout cache doesn't return stale geometry for animated elements.
        if !self.animation_overrides.is_empty() {
            fn mark_dirty(node: &mut WebCore, ids: &HashMap<u32, Vec<(String, String)>>) {
                if ids.contains_key(&node.node_id) {
                    node.layout.layout_dirty = true;
                }
                for child in &mut node.children {
                    mark_dirty(child, ids);
                }
            }
            mark_dirty(&mut self.root, &self.animation_overrides);
        }
    }
}
