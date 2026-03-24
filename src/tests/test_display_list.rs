//! Tests for the display list builder.

use crate::frame::EngineFrame;
use crate::html::parse_html;
use crate::renderer::display_list::{PaintCmd, DisplayList};
use crate::renderer::display_list_builder::build_display_list;

fn build(html: &str) -> (EngineFrame, DisplayList) {
    let doc = parse_html(html);
    let mut f = EngineFrame::new(doc, 800.0, 600.0);
    f.update_frame();
    let list = build_display_list(&f.doc.root, 800.0, 600.0);
    (f, list)
}

#[test]
fn empty_doc_produces_commands() {
    let (_, list) = build("<div style='background: white'>x</div>");
    // Should have at least some commands
    assert!(!list.is_empty(), "doc with content should produce paint commands");
}

#[test]
fn colored_div_has_fill_rect() {
    let (_, list) = build(r#"<div style="background-color: red; width: 100px; height: 50px">x</div>"#);
    let has_red_fill = list.commands.iter().any(|cmd| {
        matches!(cmd, PaintCmd::FillRect { color, .. } if color.r == 255 && color.g == 0)
    });
    assert!(has_red_fill, "red div should produce a FillRect with red color");
}

#[test]
fn text_node_produces_text_command() {
    let (_, list) = build("<p>Hello World</p>");
    let has_text = list.commands.iter().any(|cmd| {
        matches!(cmd, PaintCmd::Text { text, .. } if text.contains("Hello"))
    });
    assert!(has_text, "text content should produce a Text paint command");
}

#[test]
fn border_produces_border_command() {
    let (_, list) = build(r#"<div style="border: 2px solid blue; width: 100px; height: 50px">x</div>"#);
    let has_border = list.commands.iter().any(|cmd| {
        matches!(cmd, PaintCmd::Border { widths, .. } if widths[0] > 0.0)
    });
    assert!(has_border, "border should produce a Border command");
}

#[test]
fn display_none_produces_nothing() {
    let (_, list) = build(r#"<div style="display: none; background: red; width: 100px; height: 50px">hidden</div>"#);
    // display:none should not produce any FillRect or Text for this element
    let has_hidden_content = list.commands.iter().any(|cmd| {
        matches!(cmd, PaintCmd::Text { text, .. } if text.contains("hidden"))
    });
    assert!(!has_hidden_content, "display:none content should not be in display list");
}

#[test]
fn overflow_hidden_produces_clip() {
    let (_, list) = build(r#"<div style="overflow: hidden; width: 100px; height: 50px"><p>content</p></div>"#);
    let has_clip = list.commands.iter().any(|cmd| matches!(cmd, PaintCmd::PushClip { .. }));
    let has_pop = list.commands.iter().any(|cmd| matches!(cmd, PaintCmd::PopClip));
    assert!(has_clip, "overflow:hidden should produce PushClip");
    assert!(has_pop, "overflow:hidden should produce PopClip");
}

#[test]
fn opacity_produces_push_pop() {
    let (_, list) = build(r#"<div style="opacity: 0.5; width: 100px; height: 50px">semi</div>"#);
    let has_opacity = list.commands.iter().any(|cmd| {
        matches!(cmd, PaintCmd::PushOpacity { alpha } if (*alpha - 0.5).abs() < 0.01)
    });
    assert!(has_opacity, "opacity should produce PushOpacity");
}

#[test]
fn stacking_context_for_z_index() {
    let (_, list) = build(r#"<div style="position: relative; z-index: 5; width: 100px; height: 50px">stacked</div>"#);
    let has_ctx = list.commands.iter().any(|cmd| {
        matches!(cmd, PaintCmd::BeginStackingContext { z_index, .. } if *z_index == 5)
    });
    assert!(has_ctx, "z-index should create a stacking context");
}

#[test]
fn image_without_data_no_crash() {
    // Verify the builder handles nodes with no loaded image data (no crash)
    let (_, list) = build(r#"<div><img src="nonexistent.png" width="100" height="50"></div>"#);
    // Should not crash — may or may not produce commands depending on layout
    let _ = list.len(); // just verify we got here
}

#[test]
fn nested_elements_produce_ordered_commands() {
    let (_, list) = build(r#"
        <div style="background: blue; padding: 10px">
            <p style="background: red">text</p>
        </div>
    "#);

    // Blue fill should come before red fill (parent paints before child)
    let mut blue_idx = None;
    let mut red_idx = None;
    for (i, cmd) in list.commands.iter().enumerate() {
        if let PaintCmd::FillRect { color, .. } = cmd {
            if color.b == 255 && color.r == 0 && blue_idx.is_none() { blue_idx = Some(i); }
            if color.r == 255 && color.b == 0 && red_idx.is_none() { red_idx = Some(i); }
        }
    }
    if let (Some(b), Some(r)) = (blue_idx, red_idx) {
        assert!(b < r, "parent background should paint before child: blue@{} red@{}", b, r);
    }
}

#[test]
fn render_display_list_produces_pixels() {
    let doc = parse_html(r#"
        <div style="background: red; width: 100px; height: 50px; position: absolute; left: 10px; top: 10px">
            <p style="color: white">Hello</p>
        </div>
        <div style="background: blue; width: 200px; height: 100px; position: absolute; left: 150px; top: 10px">
            <span>World</span>
        </div>
    "#);
    let mut f = EngineFrame::new(doc, 400.0, 300.0);
    f.update_frame();

    // Render via display list path
    let mut pixmap = tiny_skia::Pixmap::new(400, 300).unwrap();
    f.engine.layout(&mut f.doc, 400.0);

    let mut renderer = crate::Renderer::new();
    renderer.render_display_list(&mut f.doc, &mut pixmap, 1.0);

    // Check that the red area has red pixels
    let data = pixmap.data();
    // Pixel at (50, 25) should be in the red div
    let idx = (25 * 400 + 50) as usize * 4;
    let (r, g, b, _a) = (data[idx], data[idx+1], data[idx+2], data[idx+3]);
    assert!(r > 200 && g < 50 && b < 50,
        "pixel at (50,25) should be red, got ({},{},{})", r, g, b);

    // Pixel at (200, 50) should be in the blue div
    let idx = (50 * 400 + 200) as usize * 4;
    let (r, g, b, _a) = (data[idx], data[idx+1], data[idx+2], data[idx+3]);
    assert!(r < 50 && g < 50 && b > 200,
        "pixel at (200,50) should be blue, got ({},{},{})", r, g, b);
}

#[test]
fn display_list_command_count_scales_with_elements() {
    let small = build("<div>one</div>").1;
    let big = build(r#"
        <div><p>a</p><p>b</p><p>c</p><p>d</p><p>e</p></div>
    "#).1;
    assert!(big.len() > small.len(),
        "more elements should produce more commands: {} vs {}", big.len(), small.len());
}
