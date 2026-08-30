//! Inheritance: which properties inherit, copying them from a parent, and
//! the animation overrides layered on top.

#![allow(unused_imports)]
use std::collections::{HashMap, HashSet};
use crate::types::*;
use super::*;

/// Walk the box tree and apply `animation_overrides` (from `Document::tick_animations`)
/// on top of the cascaded computed styles.
pub fn apply_animation_overrides(
    node:      &mut WebCore,
    overrides: &HashMap<u32, Vec<(String, String)>>,
) {
    let id = node.node_id;
    if let Some(props) = overrides.get(&id) {
        for (prop, val) in props {
            apply_property(&mut node.style, prop, val);
        }
        // Propagate inherited properties (like `color`) to descendant text nodes.
        // Text nodes have no explicit CSS rules — they inherit everything — so their
        // cloned style must track the parent's animated values.  Without this, runs
        // built by collect_items carry the stale pre-animation cascade color.
        let inherited: Vec<(&str, &str)> = props.iter()
            .filter(|(p, _)| is_inherited_css_prop(p))
            .map(|(p, v)| (p.as_str(), v.as_str()))
            .collect();
        if !inherited.is_empty() {
            propagate_to_text_descendants(&mut node.children, &inherited);
        }
    }
    for child in &mut node.children {
        apply_animation_overrides(child, overrides);
    }
}

/// CSS properties that are inherited and must be propagated to text-node descendants.
fn is_inherited_css_prop(prop: &str) -> bool {
    matches!(prop,
        "color" | "font-size" | "font-weight" | "font-style" | "font-family" |
        "letter-spacing" | "word-spacing" | "line-height" | "text-transform" |
        "text-indent" | "visibility" | "cursor"
    )
}

/// Walk `children` recursively and apply `props` to any text-node (`#text`) descendants.
/// Non-text nodes are recursed into so that nested inline elements (e.g. `<span><em>`)
/// also have their text-node leaves updated.  Nodes that have their own entry in
/// `animation_overrides` will be handled separately; here we only touch text nodes,
/// which never have transitions of their own.
fn propagate_to_text_descendants(children: &mut Vec<WebCore>, props: &[(&str, &str)]) {
    for child in children {
        if child.is_text_node() {
            for &(prop, val) in props {
                apply_property(&mut child.style, prop, val);
            }
        } else {
            propagate_to_text_descendants(&mut child.children, props);
        }
    }
}

/// Apply a single CSS property/value pair to a ComputedStyle.
/// Copy a single CSS property from parent's computed style into `style`.
/// Used when a rule declares `property: inherit` and must override a lower-specificity
/// concrete value that already changed the inherited default.
pub(crate) fn copy_property_from_parent(style: &mut ComputedStyle, parent: &ComputedStyle, prop: &str) {
    match prop {
        "font-size"       => style.font_size       = parent.font_size.clone(),
        "font-weight"     => style.font_weight     = parent.font_weight,
        "font-family"     => style.font_family     = parent.font_family.clone(),
        "font-style"      => style.font_style      = parent.font_style,
        "color"           => style.color            = parent.color,
        "line-height"     => style.line_height      = parent.line_height.clone(),
        "text-align"      => style.text_align       = parent.text_align,
        "text-decoration" => style.text_decoration  = parent.text_decoration,
        "letter-spacing"  => style.letter_spacing   = parent.letter_spacing.clone(),
        "word-spacing"    => style.word_spacing     = parent.word_spacing.clone(),
        "white-space"     => style.white_space      = parent.white_space,
        "text-transform"  => style.text_transform   = parent.text_transform,
        "direction"       => style.direction        = parent.direction,
        "visibility"      => style.visibility       = parent.visibility,
        "cursor"          => style.cursor           = parent.cursor,
        "display"         => style.display          = parent.display,
        "width"           => style.width            = parent.width.clone(),
        "height"          => style.height           = parent.height.clone(),
        "margin-top"      => style.margin_top       = parent.margin_top.clone(),
        "margin-right"    => style.margin_right     = parent.margin_right.clone(),
        "margin-bottom"   => style.margin_bottom    = parent.margin_bottom.clone(),
        "margin-left"     => style.margin_left      = parent.margin_left.clone(),
        "padding-top"     => style.padding_top      = parent.padding_top.clone(),
        "padding-right"   => style.padding_right    = parent.padding_right.clone(),
        "padding-bottom"  => style.padding_bottom   = parent.padding_bottom.clone(),
        "padding-left"    => style.padding_left     = parent.padding_left.clone(),
        "border-top-width"    => style.border_top_width    = parent.border_top_width.clone(),
        "border-right-width"  => style.border_right_width  = parent.border_right_width.clone(),
        "border-bottom-width" => style.border_bottom_width = parent.border_bottom_width.clone(),
        "border-left-width"   => style.border_left_width   = parent.border_left_width.clone(),
        "border-top-color"    => style.border_top_color    = parent.border_top_color,
        "border-right-color"  => style.border_right_color  = parent.border_right_color,
        "border-bottom-color" => style.border_bottom_color = parent.border_bottom_color,
        "border-left-color"   => style.border_left_color   = parent.border_left_color,
        "border" => {
            style.border_top_width    = parent.border_top_width.clone();
            style.border_right_width  = parent.border_right_width.clone();
            style.border_bottom_width = parent.border_bottom_width.clone();
            style.border_left_width   = parent.border_left_width.clone();
            style.border_top_color    = parent.border_top_color;
            style.border_right_color  = parent.border_right_color;
            style.border_bottom_color = parent.border_bottom_color;
            style.border_left_color   = parent.border_left_color;
            style.border_top_style    = parent.border_top_style;
            style.border_right_style  = parent.border_right_style;
            style.border_bottom_style = parent.border_bottom_style;
            style.border_left_style   = parent.border_left_style;
        }
        "margin" => {
            style.margin_top    = parent.margin_top.clone();
            style.margin_right  = parent.margin_right.clone();
            style.margin_bottom = parent.margin_bottom.clone();
            style.margin_left   = parent.margin_left.clone();
        }
        "padding" => {
            style.padding_top    = parent.padding_top.clone();
            style.padding_right  = parent.padding_right.clone();
            style.padding_bottom = parent.padding_bottom.clone();
            style.padding_left   = parent.padding_left.clone();
        }
        "background-color" => style.background_color = parent.background_color,
        "background"       => style.background_color = parent.background_color,
        "opacity"          => style.opacity     = parent.opacity,
        "overflow"         => { style.overflow_x = parent.overflow_x; style.overflow_y = parent.overflow_y; }
        "overflow-x"       => style.overflow_x  = parent.overflow_x,
        "overflow-y"       => style.overflow_y  = parent.overflow_y,
        "position"         => style.position    = parent.position,
        "float"            => style.float       = parent.float,
        "text-indent"      => style.text_indent = parent.text_indent.clone(),
        "list-style-type"  => style.list_style_type = parent.list_style_type,
        "vertical-align"   => style.vertical_align  = parent.vertical_align,
        "text-overflow"    => style.text_overflow    = parent.text_overflow,
        "word-break"       => style.word_break       = parent.word_break,
        "overflow-wrap"    => style.overflow_wrap     = parent.overflow_wrap,
        "font-stretch"     => style.font_stretch     = parent.font_stretch,
        "text-shadow"      => style.text_shadow      = parent.text_shadow.clone(),
        _ => {} // Unhandled properties — no-op
    }
}
