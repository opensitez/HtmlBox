//! Tests for the display list builder and replay.

use crate::frame::EngineFrame;
use crate::html::parse_html;
use crate::renderer::display_list::{PaintCmd, DisplayList};
use crate::renderer::display_list_builder::{build_display_list, build_display_list_full};

fn build(html: &str) -> (EngineFrame, DisplayList) {
    let doc = parse_html(html);
    let mut f = EngineFrame::new(doc, 800.0, 600.0);
    f.update_frame();
    let list = build_display_list(&f.doc.root, 800.0, 600.0);
    (f, list)
}

fn build_full(html: &str) -> (EngineFrame, DisplayList) {
    let doc = parse_html(html);
    let mut f = EngineFrame::new(doc, 800.0, 600.0);
    f.update_frame();
    let list = build_display_list_full(
        &f.doc.root, 800.0, 600.0, 0.0, 0.0, 0, 0,
        &std::collections::HashSet::new(),
    );
    (f, list)
}

// ── Basic commands ──────────────────────────────────────────────────────────

#[test]
fn non_empty_doc_produces_commands() {
    let (_, list) = build("<div style='background: white'>x</div>");
    assert!(!list.is_empty(), "doc with content should produce commands");
}

#[test]
fn colored_div_has_fill_rect() {
    let (_, list) = build(r#"<div style="background-color: red; width: 100px; height: 50px">x</div>"#);
    let has_red = list.commands.iter().any(|cmd| {
        matches!(cmd, PaintCmd::FillRect { color, .. } if color.r == 255 && color.g == 0)
    });
    assert!(has_red, "red div should produce FillRect with red");
}

#[test]
fn text_node_produces_text_command() {
    let (_, list) = build("<p>Hello World</p>");
    let has_text = list.commands.iter().any(|cmd| {
        matches!(cmd, PaintCmd::Text { text, .. } if text.contains("Hello"))
    });
    assert!(has_text, "text should produce Text command");
}

#[test]
fn border_produces_border_command() {
    let (_, list) = build(r#"<div style="border: 2px solid blue; width: 100px; height: 50px">x</div>"#);
    let has_border = list.commands.iter().any(|cmd| {
        matches!(cmd, PaintCmd::Border { widths, .. } if widths[0] > 0.0)
    });
    assert!(has_border, "border should produce Border command");
}

#[test]
fn display_none_produces_nothing() {
    let (_, list) = build(r#"<div style="display: none; background: red">hidden</div>"#);
    let has_hidden = list.commands.iter().any(|cmd| {
        matches!(cmd, PaintCmd::Text { text, .. } if text.contains("hidden"))
    });
    assert!(!has_hidden, "display:none should not produce commands");
}

// ── Clip and opacity ────────────────────────────────────────────────────────

#[test]
fn overflow_hidden_produces_clip() {
    let (_, list) = build(r#"<div style="overflow: hidden; width: 100px; height: 50px"><p>content</p></div>"#);
    let has_clip = list.commands.iter().any(|cmd| matches!(cmd, PaintCmd::PushClip { .. }));
    let has_pop = list.commands.iter().any(|cmd| matches!(cmd, PaintCmd::PopClip));
    assert!(has_clip && has_pop, "overflow:hidden should produce PushClip/PopClip");
}

#[test]
fn opacity_produces_push_pop() {
    let (_, list) = build(r#"<div style="opacity: 0.5; width: 100px; height: 50px">semi</div>"#);
    let has_op = list.commands.iter().any(|cmd| {
        matches!(cmd, PaintCmd::PushOpacity { alpha } if (*alpha - 0.5).abs() < 0.01)
    });
    assert!(has_op, "opacity should produce PushOpacity");
}

#[test]
fn stacking_context_for_z_index() {
    let (_, list) = build(r#"<div style="position: relative; z-index: 5; width: 100px; height: 50px">z</div>"#);
    let has_ctx = list.commands.iter().any(|cmd| {
        matches!(cmd, PaintCmd::BeginStackingContext { z_index, .. } if *z_index == 5)
    });
    assert!(has_ctx, "z-index should create stacking context");
}

// ── Inline content with line_cache ──────────────────────────────────────────

#[test]
fn inline_text_produces_text_commands() {
    let (_, list) = build(r#"<p style="width: 200px">This is a paragraph with some text content that should wrap.</p>"#);
    let text_cmds: Vec<_> = list.commands.iter().filter(|cmd| matches!(cmd, PaintCmd::Text { .. })).collect();
    assert!(!text_cmds.is_empty(), "paragraph text should produce Text commands");
}

#[test]
fn styled_spans_produce_separate_runs() {
    let html = r#"<p><span style="color: red">Red</span> <span style="color: blue">Blue</span></p>"#;
    let (_, list) = build(html);
    let text_cmds: Vec<_> = list.commands.iter().filter_map(|cmd| {
        if let PaintCmd::Text { text, color, .. } = cmd { Some((text.clone(), *color)) }
        else { None }
    }).collect();
    // Should have at least text commands
    assert!(!text_cmds.is_empty(), "styled spans should produce text commands");
}

// ── Hover style switching ───────────────────────────────────────────────────

#[test]
fn hover_style_applied_in_display_list() {
    let html = r#"<html><head><style>
        .btn { background-color: gray; width: 100px; height: 40px; }
        .btn:hover { background-color: red; }
    </style></head><body>
        <div class="btn" id="btn">Click</div>
    </body></html>"#;
    let doc = parse_html(html);
    let mut f = EngineFrame::new(doc, 800.0, 600.0);
    f.update_frame();

    let btn_id = f.doc.get_element_by_id("btn").unwrap();

    // Without hover
    let list_no_hover = build_display_list_full(
        &f.doc.root, 800.0, 600.0, 0.0, 0.0, 0, 0,
        &std::collections::HashSet::new(),
    );

    // With hover on button
    let list_hover = build_display_list_full(
        &f.doc.root, 800.0, 600.0, 0.0, 0.0, btn_id, 0,
        &std::collections::HashSet::new(),
    );

    // Count red FillRects in each
    let red_no_hover = list_no_hover.commands.iter().filter(|cmd| {
        matches!(cmd, PaintCmd::FillRect { color, .. } if color.r > 200 && color.g < 50)
    }).count();
    let red_hover = list_hover.commands.iter().filter(|cmd| {
        matches!(cmd, PaintCmd::FillRect { color, .. } if color.r > 200 && color.g < 50)
    }).count();

    assert!(red_hover > red_no_hover,
        "hover should add a red background: no_hover={} hover={}", red_no_hover, red_hover);
}

// ── Scrolling ───────────────────────────────────────────────────────────────

/// ⛔ The display list is SCROLL-INDEPENDENT — the scroll is applied at replay.
///
/// This test used to assert the opposite: that building at scroll 100 moved
/// the rect up by 100. That was the old contract, and it is why scrolling cost
/// seconds — a list with the offset baked in is only valid at that one offset,
/// so the page could only move by rebuilding the whole document's list. The
/// list is now built in DOCUMENT coordinates and translated by
/// `replay_with_scroll`, which is what lets one build serve every scroll
/// position.
///
/// The scroll argument survives for `position: sticky` alone — the one scheme
/// whose position really is a function of the scroll.
#[test]
fn the_display_list_is_scroll_independent() {
    let html = r#"<div style="background: blue; width: 100px; height: 50px; position: absolute; top: 200px; left: 100px">x</div>"#;
    let doc = parse_html(html);
    let mut f = EngineFrame::new(doc, 800.0, 600.0);
    f.update_frame();

    let unscrolled = build_display_list_full(
        &f.doc.root, 800.0, 600.0, 0.0, 0.0, 0, 0,
        &std::collections::HashSet::new(),
    );
    let scrolled = build_display_list_full(
        &f.doc.root, 800.0, 600.0, 0.0, 100.0, 0, 0,
        &std::collections::HashSet::new(),
    );

    fn find_blue_y(list: &DisplayList) -> Option<f32> {
        for cmd in &list.commands {
            if let PaintCmd::FillRect { rect, color, .. } = cmd {
                if color.b > 200 && color.r < 50 { return Some(rect.y); }
            }
        }
        None
    }
    let y1 = find_blue_y(&unscrolled).expect("the blue box is painted");
    let y2 = find_blue_y(&scrolled).expect("the blue box is painted");

    assert!(
        (y1 - y2).abs() < 0.01,
        "the same box must build to the same document position whatever the \
         scroll — got {y1} and {y2}; a scroll-dependent list cannot be cached"
    );
    assert!((y1 - 200.0).abs() < 2.0, "and that position is the DOCUMENT one");
}

// ── Pseudo-elements ─────────────────────────────────────────────────────────

#[test]
fn before_after_pseudo_elements() {
    let html = r#"<html><head><style>
        .with-before::before { content: ">>"; color: red; }
    </style></head><body>
        <p class="with-before">Text</p>
    </body></html>"#;
    let (_, list) = build(html);
    // Should have text commands (the ::before content and the main text)
    let text_cmds: Vec<_> = list.commands.iter().filter(|cmd| matches!(cmd, PaintCmd::Text { .. })).collect();
    assert!(!text_cmds.is_empty(), "::before should produce text commands");
}

// ── Box shadow ──────────────────────────────────────────────────────────────

#[test]
fn box_shadow_produces_command() {
    let (_, list) = build(r#"<div style="box-shadow: 5px 5px 10px black; width: 100px; height: 50px">x</div>"#);
    let has_shadow = list.commands.iter().any(|cmd| matches!(cmd, PaintCmd::BoxShadow { .. }));
    assert!(has_shadow, "box-shadow should produce BoxShadow command");
}

// ── Pixel-level rendering tests ─────────────────────────────────────────────

#[test]
fn render_display_list_produces_colored_pixels() {
    let doc = parse_html(r#"
        <div style="background: red; width: 100px; height: 50px; position: absolute; left: 10px; top: 10px">x</div>
        <div style="background: blue; width: 200px; height: 100px; position: absolute; left: 150px; top: 10px">x</div>
    "#);
    let mut f = EngineFrame::new(doc, 400.0, 300.0);
    f.update_frame();

    let mut pixmap = tiny_skia::Pixmap::new(400, 300).unwrap();
    let mut renderer = crate::Renderer::new();
    renderer.render(&mut f.doc, &mut pixmap, 1.0);

    let data = pixmap.data();
    // Red at (50, 25)
    let idx = (25 * 400 + 50) as usize * 4;
    assert!(data[idx] > 200 && data[idx+1] < 50,
        "pixel (50,25) should be red: ({},{},{})", data[idx], data[idx+1], data[idx+2]);
    // Blue at (200, 50)
    let idx = (50 * 400 + 200) as usize * 4;
    assert!(data[idx+2] > 200 && data[idx] < 50,
        "pixel (200,50) should be blue: ({},{},{})", data[idx], data[idx+1], data[idx+2]);
}

#[test]
fn render_display_list_text_produces_dark_pixels() {
    let doc = parse_html(r#"<p style="color: black; font-size: 20px; position: absolute; left: 10px; top: 10px">Hello World</p>"#);
    let mut f = EngineFrame::new(doc, 400.0, 300.0);
    f.update_frame();

    let mut pixmap = tiny_skia::Pixmap::new(400, 300).unwrap();
    let mut renderer = crate::Renderer::new();
    renderer.render(&mut f.doc, &mut pixmap, 1.0);

    let data = pixmap.data();
    let mut has_dark = false;
    // Scan a wider area — absolute positioning + font metrics may shift text
    for y in 0..150 {
        for x in 0..300 {
            let idx = (y * 400 + x) as usize * 4;
            if data[idx] < 100 && data[idx+1] < 100 && data[idx+2] < 100 {
                has_dark = true;
                break;
            }
        }
        if has_dark { break; }
    }
    assert!(has_dark, "text should produce dark pixels");
}

#[test]
fn render_display_list_border_visible() {
    let doc = parse_html(r#"<div style="border: 3px solid green; width: 100px; height: 50px; position: absolute; left: 50px; top: 50px">x</div>"#);
    let mut f = EngineFrame::new(doc, 400.0, 300.0);
    f.update_frame();

    let mut pixmap = tiny_skia::Pixmap::new(400, 300).unwrap();
    let mut renderer = crate::Renderer::new();
    renderer.render(&mut f.doc, &mut pixmap, 1.0);

    let data = pixmap.data();
    // Top border at (100, 50) should be greenish
    let idx = (50 * 400 + 100) as usize * 4;
    assert!(data[idx+1] > 100, "top border should have green: g={}", data[idx+1]);
}

// ── Command ordering ────────────────────────────────────────────────────────

#[test]
fn parent_background_before_child() {
    let (_, list) = build(r#"
        <div style="background: blue; padding: 10px">
            <p style="background: red">text</p>
        </div>
    "#);
    let mut blue_idx = None;
    let mut red_idx = None;
    for (i, cmd) in list.commands.iter().enumerate() {
        if let PaintCmd::FillRect { color, .. } = cmd {
            if color.b == 255 && color.r == 0 && blue_idx.is_none() { blue_idx = Some(i); }
            if color.r == 255 && color.b == 0 && red_idx.is_none() { red_idx = Some(i); }
        }
    }
    if let (Some(b), Some(r)) = (blue_idx, red_idx) {
        assert!(b < r, "parent paints before child: blue@{} red@{}", b, r);
    }
}

#[test]
fn command_count_scales_with_elements() {
    let small = build("<div>one</div>").1;
    let big = build("<div><p>a</p><p>b</p><p>c</p><p>d</p><p>e</p></div>").1;
    assert!(big.len() > small.len(), "more elements = more commands: {} vs {}", big.len(), small.len());
}

// ── No crash tests ──────────────────────────────────────────────────────────

#[test]
fn image_without_data_no_crash() {
    let (_, list) = build("<div><img src='x.png' width='100' height='50'></div>");
    let _ = list.len();
}

#[test]
fn deeply_nested_no_crash() {
    let html = "<div>".repeat(10) + "deep" + &"</div>".repeat(10);
    let (_, list) = build(&html);
    assert!(!list.is_empty());
}

#[test]
fn empty_text_no_crash() {
    let (_, list) = build("<p></p><p>   </p><p>\n</p>");
    let _ = list.len();
}

// ── object-fit / object-position (css-images-3 §5.5) ────────────────────────

/// Give every `<img>` a natural size, then rebuild the list.
fn build_with_image(html: &str, iw: u32, ih: u32) -> DisplayList {
    let mut doc = parse_html(html);
    fn seed(n: &mut crate::types::WebCore, iw: u32, ih: u32) {
        if n.tag == "img" {
            n.image_width = iw;
            n.image_height = ih;
            n.image_data = Some(vec![255u8; (iw * ih * 4) as usize]);
        }
        for c in &mut n.children { seed(c, iw, ih); }
    }
    seed(&mut doc.root, iw, ih);
    let mut f = EngineFrame::new(doc, 800.0, 600.0);
    f.update_frame();
    build_display_list(&f.doc.root, 800.0, 600.0)
}

fn image_rect(list: &DisplayList) -> Option<crate::types::Rect> {
    list.commands.iter().find_map(|c| match c {
        PaintCmd::Image { rect, .. } => Some(*rect),
        _ => None,
    })
}

/// **`object-fit` decides the drawn size of a replaced element** (css-images-3
/// §5.5). It was parsed into `ComputedStyle` and never read by the paint path,
/// so every image was stretched to its content box — the `fill` behaviour —
/// whatever the author asked for.
#[test]
fn object_fit_cover_preserves_the_aspect_ratio() {
    // 200x100 natural, shown in a 100x100 box.
    let list = build_with_image(
        "<style>*{margin:0;padding:0} img{width:100px;height:100px;object-fit:cover}</style><img src=x>",
        200, 100);
    let r = image_rect(&list).expect("an image was painted");
    // cover → scale = max(100/200, 100/100) = 1 → 200x100, centred horizontally.
    assert_eq!((r.w, r.h), (200.0, 100.0), "cover must keep the 2:1 ratio, got {}x{}", r.w, r.h);
    assert_eq!(r.x, -50.0, "centred by the default object-position: 50%");
}

#[test]
fn object_fit_contain_fits_inside_the_box() {
    let list = build_with_image(
        "<style>*{margin:0;padding:0} img{width:100px;height:100px;object-fit:contain}</style><img src=x>",
        200, 100);
    let r = image_rect(&list).expect("an image was painted");
    // contain → scale = min(100/200, 100/100) = 0.5 → 100x50, centred vertically.
    assert_eq!((r.w, r.h), (100.0, 50.0), "got {}x{}", r.w, r.h);
    assert_eq!(r.y, 25.0, "centred vertically");
}

#[test]
fn object_fit_none_uses_the_natural_size() {
    let list = build_with_image(
        "<style>*{margin:0;padding:0} img{width:100px;height:100px;object-fit:none}</style><img src=x>",
        200, 100);
    let r = image_rect(&list).expect("an image was painted");
    assert_eq!((r.w, r.h), (200.0, 100.0), "got {}x{}", r.w, r.h);
}

/// `fill` is the initial value and must still stretch to the box.
#[test]
fn object_fit_fill_is_the_default_and_stretches() {
    let list = build_with_image(
        "<style>*{margin:0;padding:0} img{width:100px;height:100px}</style><img src=x>",
        200, 100);
    let r = image_rect(&list).expect("an image was painted");
    assert_eq!((r.w, r.h), (100.0, 100.0), "got {}x{}", r.w, r.h);
}

/// `object-position` places the object in the box.
#[test]
fn object_position_places_the_object() {
    let list = build_with_image(
        "<style>*{margin:0;padding:0} img{width:100px;height:100px;object-fit:contain;object-position:left top}</style><img src=x>",
        200, 100);
    let r = image_rect(&list).expect("an image was painted");
    assert_eq!((r.x, r.y), (0.0, 0.0), "left top pins to the origin, got {},{}", r.x, r.y);
}

// ── Gradient parsing: colour-stop fixup and layers (css-images-3 §4.3.1) ─────

fn gradient_stops(list: &DisplayList) -> Option<Vec<(crate::types::Color, f32)>> {
    list.commands.iter().find_map(|cmd| match cmd {
        PaintCmd::Gradient { stops, .. } => Some(stops.clone()),
        _ => None,
    })
}

fn gradient_rect(list: &DisplayList) -> Option<crate::types::Rect> {
    list.commands.iter().find_map(|cmd| match cmd {
        PaintCmd::Gradient { rect, .. } => Some(*rect),
        _ => None,
    })
}

fn fill_rect_of(list: &DisplayList, r: u8, g: u8, b: u8) -> Option<crate::types::Rect> {
    list.commands.iter().find_map(|cmd| match cmd {
        PaintCmd::FillRect { rect, color, .. }
            if color.r == r && color.g == g && color.b == b => Some(*rect),
        _ => None,
    })
}

#[test]
fn a_second_background_layer_does_not_corrupt_the_first() {
    let (_, list) = build(r#"<div style="width:100px;height:50px;background:linear-gradient(red,blue),linear-gradient(lime,black)">x</div>"#);
    let stops = gradient_stops(&list).expect("a gradient was painted");
    assert_eq!(stops.len(), 2, "the first layer has two stops, got {stops:?}");
    assert_eq!((stops[0].0.r, stops[0].0.b), (255, 0), "first stop is red, got {:?}", stops[0].0);
    assert_eq!((stops[1].0.r, stops[1].0.b), (0, 255), "second stop is blue, got {:?}", stops[1].0);
    assert!((stops[1].1 - 1.0).abs() < 0.001, "second stop sits at 100%, got {}", stops[1].1);
}

#[test]
fn a_radial_gradient_keeps_a_positioned_first_stop() {
    let (_, list) = build(r#"<div style="width:100px;height:50px;background:radial-gradient(red 10%, blue 90%)">x</div>"#);
    let stops = gradient_stops(&list).expect("a gradient was painted");
    assert_eq!(stops.len(), 2, "both stops survive, got {stops:?}");
    assert_eq!((stops[0].0.r, stops[0].0.b), (255, 0), "first stop is red, got {:?}", stops[0].0);
    assert!((stops[0].1 - 0.10).abs() < 0.001, "first stop sits at 10%, got {}", stops[0].1);
    assert!((stops[1].1 - 0.90).abs() < 0.001, "second stop sits at 90%, got {}", stops[1].1);
}

#[test]
fn a_radial_gradient_descriptor_does_not_eat_a_stop() {
    let (_, list) = build(r#"<div style="width:100px;height:50px;background:radial-gradient(circle at 50% 50%, rgba(255,0,0,1) 0%, rgb(0,0,255) 100%)">x</div>"#);
    let stops = gradient_stops(&list).expect("a gradient was painted");
    assert_eq!(stops.len(), 2, "the descriptor is dropped and both stops parse, got {stops:?}");
    assert_eq!((stops[0].0.r, stops[0].0.b), (255, 0), "first stop is red, got {:?}", stops[0].0);
    assert_eq!((stops[1].0.r, stops[1].0.b), (0, 255), "second stop is blue, got {:?}", stops[1].0);
}

#[test]
fn a_stop_before_its_predecessor_is_clamped_up_to_it() {
    // css-images-3 §4.3.1 fixup step 3.
    let (_, list) = build(r#"<div style="width:100px;height:50px;background:linear-gradient(red 60%, blue 20%, lime 90%)">x</div>"#);
    let stops = gradient_stops(&list).expect("a gradient was painted");
    assert_eq!(stops.len(), 3, "three stops, got {stops:?}");
    assert!((stops[1].1 - 0.60).abs() < 0.001, "20% clamps up to the preceding 60%, got {}", stops[1].1);
    assert!((stops[2].1 - 0.90).abs() < 0.001, "90% is untouched, got {}", stops[2].1);
}

#[test]
fn unpositioned_stops_space_between_positioned_neighbours() {
    // css-images-3 §4.3.1 fixup step 4 — the spec's own example 2.
    let (_, list) = build(r#"<div style="width:100px;height:50px;background:linear-gradient(red 40%, white, black, blue)">x</div>"#);
    let stops = gradient_stops(&list).expect("a gradient was painted");
    assert_eq!(stops.len(), 4, "four stops, got {stops:?}");
    for (i, want) in [0.40f32, 0.60, 0.80, 1.0].iter().enumerate() {
        assert!((stops[i].1 - want).abs() < 0.001,
            "stop {i} sits at {want}, got {} (all: {stops:?})", stops[i].1);
    }
}

#[test]
fn a_double_position_stop_expands_to_two_stops() {
    // css-images-3 §4.3.1: <linear-color-stop> = <color> <length-percentage>{1,2}
    let (_, list) = build(r#"<div style="width:100px;height:50px;background:linear-gradient(red 10% 20%, blue)">x</div>"#);
    let stops = gradient_stops(&list).expect("a gradient was painted");
    assert_eq!(stops.len(), 3, "the shorthand expands to two red stops plus blue, got {stops:?}");
    assert!((stops[0].1 - 0.10).abs() < 0.001, "first red at 10%, got {}", stops[0].1);
    assert!((stops[1].1 - 0.20).abs() < 0.001, "second red at 20%, got {}", stops[1].1);
    assert_eq!((stops[1].0.r, stops[1].0.b), (255, 0), "the second stop repeats the colour, got {:?}", stops[1].0);
}

// ── background-clip / background-origin (css-backgrounds-3 §3.7, §3.6) ──────

#[test]
fn the_background_colour_fills_the_border_box_by_default() {
    // background-clip's initial value is border-box, so the colour bleeds under
    // a transparent border instead of stopping at the padding edge.
    let (_, list) = build(r#"<style>*{margin:0;padding:0}</style><div style="width:100px;height:50px;border:10px solid transparent;background-color:red">x</div>"#);
    let r = fill_rect_of(&list, 255, 0, 0).expect("a red background was painted");
    assert_eq!((r.w, r.h), (120.0, 70.0), "the painting area is the border box, got {}x{}", r.w, r.h);
}

#[test]
fn background_clip_padding_box_stops_at_the_padding_edge() {
    let (_, list) = build(r#"<style>*{margin:0;padding:0}</style><div style="width:100px;height:50px;border:10px solid transparent;background-color:red;background-clip:padding-box">x</div>"#);
    let r = fill_rect_of(&list, 255, 0, 0).expect("a red background was painted");
    assert_eq!((r.w, r.h), (100.0, 50.0), "the painting area is the padding box, got {}x{}", r.w, r.h);
}

#[test]
fn background_clip_content_box_stops_at_the_content_edge() {
    let (_, list) = build(r#"<style>*{margin:0;padding:0}</style><div style="width:100px;height:50px;padding:10px;background-color:red;background-clip:content-box">x</div>"#);
    let r = fill_rect_of(&list, 255, 0, 0).expect("a red background was painted");
    assert_eq!((r.w, r.h), (100.0, 50.0), "the painting area is the content box, got {}x{}", r.w, r.h);
}

#[test]
fn a_gradient_is_positioned_in_the_padding_box_by_default() {
    // background-origin's initial value is padding-box — the border box is the
    // CLIP, not the positioning area.
    let (_, list) = build(r#"<style>*{margin:0;padding:0}</style><div style="width:100px;height:50px;border:10px solid transparent;background:linear-gradient(red,blue)">x</div>"#);
    let r = gradient_rect(&list).expect("a gradient was painted");
    assert_eq!((r.w, r.h), (100.0, 50.0), "the positioning area is the padding box, got {}x{}", r.w, r.h);
}

#[test]
fn background_origin_border_box_sizes_the_gradient_to_the_border_box() {
    let (_, list) = build(r#"<style>*{margin:0;padding:0}</style><div style="width:100px;height:50px;border:10px solid transparent;background:linear-gradient(red,blue);background-origin:border-box">x</div>"#);
    let r = gradient_rect(&list).expect("a gradient was painted");
    assert_eq!((r.w, r.h), (120.0, 70.0), "the positioning area is the border box, got {}x{}", r.w, r.h);
}

#[test]
fn a_gradient_repeats_under_a_transparent_border() {
    // The gradient image is the size of the POSITIONING area (padding box) and
    // tiles across the PAINTING area (border box), so the strip under a
    // transparent border shows the next repetition — not a gap.
    // Chrome renders (60,10) as rgb(24,0,230) and (60,130) as rgb(228,0,26)
    // for this fixture.
    let doc = parse_html(r#"<style>body{margin:0}</style>
        <div style="position:absolute;left:0;top:0;width:100px;height:100px;
                    border:20px solid transparent;
                    background:linear-gradient(to bottom,#ff0000,#0000ff)"></div>"#);
    let mut f = EngineFrame::new(doc, 400.0, 300.0);
    f.update_frame();
    let mut pixmap = tiny_skia::Pixmap::new(400, 300).unwrap();
    let mut renderer = crate::Renderer::new();
    renderer.render(&mut f.doc, &mut pixmap, 1.0);
    let data = pixmap.data();
    let px = |x: usize, y: usize| {
        let i = (y * 400 + x) * 4;
        (data[i], data[i + 1], data[i + 2])
    };
    let top = px(60, 10);
    assert!(top.2 > 180 && top.0 < 80,
        "the top border strip repeats the END of the gradient, got {top:?}");
    let bottom = px(60, 130);
    assert!(bottom.0 > 180 && bottom.2 < 80,
        "the bottom border strip repeats the START of the gradient, got {bottom:?}");
    let inside = px(60, 25);
    assert!(inside.0 > 180 && inside.2 < 80,
        "the top of the padding box is still the first stop, got {inside:?}");
}

// ── border-radius per corner (css-borders-4) ────────────────────────────────

fn first_radii(list: &DisplayList) -> Option<[f32; 4]> {
    list.commands.iter().find_map(|c| match c {
        PaintCmd::FillRect { radius, .. } => Some(*radius),
        _ => None,
    })
}

/// **Each corner keeps its own radius.** `border_radius` is only a mirror of
/// the top-left longhand, not a "the shorthand was used" flag, but the painter
/// treated any non-zero top-left as "apply this to all four corners". The
/// top-rounded card — `border-radius: 16px 16px 0 0` — came out rounded on all
/// four, and the same corrupted array feeds the border stroke and the
/// overflow clip.
#[test]
fn border_radius_keeps_each_corner_distinct() {
    let (_f, list) = build(
        "<style>* { margin:0; padding:0 }\
         div { width:200px; height:100px; background:red;\
               border-radius: 16px 16px 0 0 }</style><div></div>");
    let r = first_radii(&list).expect("a background rect was painted");
    assert_eq!(r, [16.0, 16.0, 0.0, 0.0], "got {r:?}");
}

/// A single-value shorthand still rounds every corner.
#[test]
fn border_radius_shorthand_still_rounds_all_corners() {
    let (_f, list) = build(
        "<style>* { margin:0; padding:0 }\
         div { width:200px; height:100px; background:red; border-radius: 12px }</style><div></div>");
    let r = first_radii(&list).expect("a background rect was painted");
    assert_eq!(r, [12.0, 12.0, 12.0, 12.0], "got {r:?}");
}
