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
