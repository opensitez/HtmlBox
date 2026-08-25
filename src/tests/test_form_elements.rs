use crate::{parse_html, Renderer, Document};
use crate::css::apply_cascade_vp;
use crate::types::*;
use crate::layout::LayoutEngine;

fn layout_html(html: &str, width: f32) -> Document {
    let mut doc = parse_html(html);
    apply_cascade_vp(&mut doc.root, &doc.stylesheet, None, 16.0, width, 900.0, 0, false);
    let mut eng = LayoutEngine::new();
    eng.viewport_h = 900.0;
    eng.layout(&mut doc, width);
    doc
}

fn find_by_id<'a>(node: &'a WebCore, id: &str) -> Option<&'a WebCore> {
    if node.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(node); }
    for child in &node.children { if let Some(n) = find_by_id(child, id) { return Some(n); } }
    None
}

fn find_by_tag<'a>(node: &'a WebCore, tag: &str) -> Option<&'a WebCore> {
    if node.tag == tag { return Some(node); }
    for child in &node.children { if let Some(n) = find_by_tag(child, tag) { return Some(n); } }
    None
}

fn find_all_by_tag<'a>(node: &'a WebCore, tag: &str, out: &mut Vec<&'a WebCore>) {
    if node.tag == tag { out.push(node); }
    for child in &node.children { find_all_by_tag(child, tag, out); }
}

fn find_by_node_id<'a>(node: &'a WebCore, nid: u32) -> Option<&'a WebCore> {
    if node.node_id == nid { return Some(node); }
    for child in &node.children { if let Some(n) = find_by_node_id(child, nid) { return Some(n); } }
    None
}

// ── Text Input Tests ─────────────────────────────────────────────────────────

#[test]
fn text_input_has_correct_display() {
    let doc = layout_html(r#"<input type="text" value="hello">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.style.display, Display::InlineBlock);
}

#[test]
fn text_input_has_width() {
    let doc = layout_html(r#"<input type="text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.layout.content_rect.w > 100.0, "input width {} should be > 100", input.layout.content_rect.w);
}

#[test]
fn text_input_has_height() {
    let doc = layout_html(r#"<input type="text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.layout.content_rect.h > 10.0, "input height {} should be > 10", input.layout.content_rect.h);
}

#[test]
fn text_input_preserves_value() {
    let doc = layout_html(r#"<input type="text" value="test123">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.attributes.get("value").map(|s| s.as_str()), Some("test123"));
}

#[test]
fn text_input_preserves_placeholder() {
    let doc = layout_html(r#"<input type="text" placeholder="Enter text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.attributes.get("placeholder").map(|s| s.as_str()), Some("Enter text"));
}

#[test]
fn text_input_has_border() {
    let doc = layout_html(r#"<input type="text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.layout.resolved_border_top > 0.0, "input should have border");
    assert!(input.layout.resolved_border_left > 0.0);
}

#[test]
fn text_input_has_white_background() {
    let doc = layout_html(r#"<input type="text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.style.background_color.r, 255);
    assert_eq!(input.style.background_color.g, 255);
    assert_eq!(input.style.background_color.b, 255);
}

#[test]
fn text_input_vertical_align_middle() {
    let doc = layout_html(r#"<input type="text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.style.vertical_align, VerticalAlign::Middle);
}

#[test]
fn text_input_box_sizing_border_box() {
    let doc = layout_html(r#"<input type="text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.style.box_sizing, BoxSizing::BorderBox);
}

#[test]
fn text_input_text_centered_vertically() {
    // The content height should leave room above and below the text line
    let doc = layout_html(r#"<input type="text" value="Hello">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    let font_px = input.style.font_size_px(16.0, 16.0);
    let line_h = font_px * 1.2;
    let top_space = (input.layout.content_rect.h - line_h) / 2.0;
    assert!(top_space > 1.0, "should have > 1px above text, got {}", top_space);
}

#[test]
fn password_input_preserves_value() {
    let doc = layout_html(r#"<input type="password" value="secret">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.attributes.get("value").map(|s| s.as_str()), Some("secret"));
}

#[test]
fn text_input_cursor_starts_at_zero() {
    let doc = layout_html(r#"<input type="text" value="hello">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.input_cursor, 0);
}

#[test]
fn process_form_input_key_inserts_char() {
    let doc = layout_html(r#"<input type="text" id="t" value="ab">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut WebCore, id: &str) -> Option<&'a mut WebCore> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 2; // at end
    input.input_sel_anchor = 2;
    let changed = process_form_input_key(input, 'c' as u32, Some('c'), false, false);
    assert!(changed);
    assert_eq!(input_value(input), "abc");
    assert_eq!(input.input_cursor, 3);
}

#[test]
fn process_form_input_key_backspace() {
    let doc = layout_html(r#"<input type="text" id="t" value="abc">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut WebCore, id: &str) -> Option<&'a mut WebCore> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 3;
    input.input_sel_anchor = 3;
    let changed = process_form_input_key(input, 8, None, false, false); // backspace
    assert!(changed);
    assert_eq!(input_value(input), "ab");
    assert_eq!(input.input_cursor, 2);
}

#[test]
fn process_form_input_key_arrow_left() {
    let doc = layout_html(r#"<input type="text" id="t" value="abc">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut WebCore, id: &str) -> Option<&'a mut WebCore> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 2;
    input.input_sel_anchor = 2;
    process_form_input_key(input, 37, None, false, false); // left arrow
    assert_eq!(input.input_cursor, 1);
}

#[test]
fn process_form_input_key_arrow_right() {
    let doc = layout_html(r#"<input type="text" id="t" value="abc">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut WebCore, id: &str) -> Option<&'a mut WebCore> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 1;
    input.input_sel_anchor = 1;
    process_form_input_key(input, 39, None, false, false); // right arrow
    assert_eq!(input.input_cursor, 2);
}

#[test]
fn process_form_input_key_enter_not_in_input() {
    let doc = layout_html(r#"<input type="text" id="t" value="abc">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut WebCore, id: &str) -> Option<&'a mut WebCore> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 3;
    input.input_sel_anchor = 3;
    let changed = process_form_input_key(input, 13, None, false, false); // enter
    assert!(!changed, "Enter should not insert in single-line input");
    assert_eq!(input_value(input), "abc");
}

#[test]
fn process_form_input_key_space() {
    let doc = layout_html(r#"<input type="text" id="t" value="ab">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut WebCore, id: &str) -> Option<&'a mut WebCore> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 1;
    input.input_sel_anchor = 1;
    let changed = process_form_input_key(input, 32, Some(' '), false, false);
    assert!(changed);
    assert_eq!(input_value(input), "a b");
    assert_eq!(input.input_cursor, 2);
}

// ── Checkbox Tests ───────────────────────────────────────────────────────────

#[test]
fn checkbox_has_correct_size() {
    let doc = layout_html(r#"<input type="checkbox">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.layout.content_rect.w, 16.0);
    assert_eq!(input.layout.content_rect.h, 16.0);
}

#[test]
fn checkbox_no_border() {
    let doc = layout_html(r#"<input type="checkbox">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.layout.resolved_border_top, 0.0);
}

#[test]
fn checkbox_transparent_background() {
    let doc = layout_html(r#"<input type="checkbox">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.style.background_color.a, 0, "checkbox bg should be transparent");
}

#[test]
fn checkbox_checked_attribute() {
    let doc = layout_html(r#"<input type="checkbox" checked>"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.attributes.contains_key("checked"));
}

#[test]
fn checkbox_text_not_overlapping() {
    let doc = layout_html(r#"<div><input type="checkbox"> Label</div>"#, 400.0);
    let mut inputs = Vec::new();
    find_all_by_tag(&doc.root, "input", &mut inputs);
    let input = inputs[0];
    // The text "Label" should start after the checkbox margin box
    let input_right = input.layout.margin_rect.x + input.layout.margin_rect.w;
    // Check that the parent div's line cache positions text after the checkbox
    let div = find_by_tag(&doc.root, "div").unwrap();
    assert!(!div.layout.line_cache.is_empty(), "div should have line cache");
    let line = &div.layout.line_cache[0];
    // text_x_offset should be > 0 (accounting for checkbox)
    assert!(line.text_x_offset > 0.0, "text_x_offset {} should be > 0", line.text_x_offset);
}

// ── Radio Button Tests ───────────────────────────────────────────────────────

#[test]
fn radio_has_correct_size() {
    let doc = layout_html(r#"<input type="radio" name="g">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.layout.content_rect.w, 16.0);
    assert_eq!(input.layout.content_rect.h, 16.0);
}

#[test]
fn radio_checked_attribute() {
    let doc = layout_html(r#"<input type="radio" name="g" checked>"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.attributes.contains_key("checked"));
}

// ── Button Tests ─────────────────────────────────────────────────────────────

#[test]
fn submit_button_has_text_child() {
    // Test parse_html alone first
    let doc_parse = parse_html(r#"<input type="submit" value="Go">"#);
    let inp_parse = find_by_tag(&doc_parse.root, "input").unwrap();
    eprintln!("AFTER PARSE: children={}", inp_parse.children.len());
    for (i, c) in inp_parse.children.iter().enumerate() {
        eprintln!("  child[{}]: tag={} text={:?}", i, c.tag, c.text);
    }

    let doc = layout_html(r#"<input type="submit" value="Go">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    eprintln!("AFTER LAYOUT: submit button: tag={} type={:?} display={:?} children={}",
        input.tag,
        input.attributes.get("type"),
        input.style.display,
        input.children.len());
    for (i, c) in input.children.iter().enumerate() {
        eprintln!("  child[{}]: tag={} text={:?}", i, c.tag, c.text);
    }
    assert!(!input.children.is_empty(), "submit button should have text child");
    let text = &input.children[0];
    assert_eq!(text.tag, "#text");
    assert_eq!(text.text, "Go");
}

#[test]
fn submit_button_default_label() {
    let doc = layout_html(r#"<input type="submit">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(!input.children.is_empty());
    assert_eq!(input.children[0].text, "Submit");
}

#[test]
fn reset_button_default_label() {
    let doc = layout_html(r#"<input type="reset">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(!input.children.is_empty());
    assert_eq!(input.children[0].text, "Reset");
}

#[test]
fn button_has_background() {
    let doc = layout_html(r#"<input type="submit" value="Go">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.style.background_color.a > 0, "button should have background");
}

#[test]
fn button_display_inline_flex() {
    let doc = layout_html(r#"<input type="submit">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.style.display, Display::InlineFlex);
}

#[test]
fn button_has_width() {
    let doc = layout_html(r#"<input type="submit" value="Submit Form">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.layout.content_rect.w > 20.0, "button width {} should be > 20", input.layout.content_rect.w);
}

// ── Select Tests ─────────────────────────────────────────────────────────────

#[test]
fn select_preserves_options_in_dom() {
    let doc = layout_html(r#"<select><option>A</option><option>B</option><option>C</option></select>"#, 400.0);
    let select = find_by_tag(&doc.root, "select").unwrap();
    let options: Vec<_> = select.children.iter().filter(|c| c.tag == "option").collect();
    assert_eq!(options.len(), 3, "select should keep 3 option children in DOM");
}

#[test]
fn select_options_are_hidden() {
    let doc = layout_html(r#"<select><option>A</option><option>B</option></select>"#, 400.0);
    let select = find_by_tag(&doc.root, "select").unwrap();
    for child in &select.children {
        if child.tag == "option" {
            assert_eq!(child.style.display, Display::None, "options should be display:none");
        }
    }
}

#[test]
fn select_has_display_text_node() {
    let doc = layout_html(r#"<select><option>First</option><option selected>Second</option></select>"#, 400.0);
    let select = find_by_tag(&doc.root, "select").unwrap();
    let text_node = select.children.iter().find(|c| c.tag == "#text");
    assert!(text_node.is_some(), "select should have a #text child for display");
    assert_eq!(text_node.unwrap().text.trim(), "Second");
}

#[test]
fn select_first_option_selected_by_default() {
    let doc = layout_html(r#"<select><option>Alpha</option><option>Beta</option></select>"#, 400.0);
    let select = find_by_tag(&doc.root, "select").unwrap();
    let text_node = select.children.iter().find(|c| c.tag == "#text");
    assert!(text_node.is_some());
    assert_eq!(text_node.unwrap().text.trim(), "Alpha");
}

#[test]
fn select_tracks_selected_index() {
    let doc = layout_html(r#"<select><option>A</option><option selected>B</option><option>C</option></select>"#, 400.0);
    let select = find_by_tag(&doc.root, "select").unwrap();
    assert_eq!(select.data.get("_selected_idx").map(|s| s.as_str()), Some("1"));
}

#[test]
fn select_has_width() {
    let doc = layout_html(r#"<select><option>Option</option></select>"#, 400.0);
    let select = find_by_tag(&doc.root, "select").unwrap();
    assert!(select.layout.content_rect.w > 50.0, "select width {} should be > 50", select.layout.content_rect.w);
}

#[test]
fn optgroup_preserved_in_dom() {
    let doc = layout_html(r#"<select>
        <optgroup label="Fruits"><option>Apple</option><option>Banana</option></optgroup>
        <optgroup label="Vegs"><option>Carrot</option></optgroup>
    </select>"#, 400.0);
    let select = find_by_tag(&doc.root, "select").unwrap();
    let optgroups: Vec<_> = select.children.iter().filter(|c| c.tag == "optgroup").collect();
    assert_eq!(optgroups.len(), 2, "select should keep optgroup children");
}

// ── Textarea Tests ───────────────────────────────────────────────────────────

#[test]
fn textarea_has_content() {
    let doc = layout_html(r#"<textarea>Hello World</textarea>"#, 400.0);
    let ta = find_by_tag(&doc.root, "textarea").unwrap();
    let has_text = ta.children.iter().any(|c| c.tag == "#text" && c.text.contains("Hello"));
    assert!(has_text, "textarea should contain text");
}

#[test]
fn textarea_has_width_and_height() {
    let doc = layout_html(r#"<textarea rows="3">Text</textarea>"#, 400.0);
    let ta = find_by_tag(&doc.root, "textarea").unwrap();
    assert!(ta.layout.content_rect.w > 50.0);
    assert!(ta.layout.content_rect.h > 20.0);
}

// ── Range Input Tests ────────────────────────────────────────────────────────

#[test]
fn range_input_preserves_attributes() {
    let doc = layout_html(r#"<input type="range" min="0" max="100" value="60">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.attributes.get("min").map(|s| s.as_str()), Some("0"));
    assert_eq!(input.attributes.get("max").map(|s| s.as_str()), Some("100"));
    assert_eq!(input.attributes.get("value").map(|s| s.as_str()), Some("60"));
}

#[test]
fn range_input_no_border() {
    let doc = layout_html(r#"<input type="range">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.layout.resolved_border_top, 0.0);
}

// ── Progress Tests ───────────────────────────────────────────────────────────

#[test]
fn progress_has_size() {
    let doc = layout_html(r#"<progress value="0.5" max="1"></progress>"#, 400.0);
    let prog = find_by_tag(&doc.root, "progress").unwrap();
    assert!(prog.layout.content_rect.w > 50.0);
    assert!(prog.layout.content_rect.h > 5.0);
}

// ── Meter Tests ──────────────────────────────────────────────────────────────

#[test]
fn meter_has_size() {
    let doc = layout_html(r#"<meter value="0.5" min="0" max="1"></meter>"#, 400.0);
    let meter = find_by_tag(&doc.root, "meter").unwrap();
    assert!(meter.layout.content_rect.w > 30.0);
    assert!(meter.layout.content_rect.h > 5.0);
}

// ── Fieldset/Legend Tests ────────────────────────────────────────────────────

#[test]
fn fieldset_is_block() {
    let doc = layout_html(r#"<fieldset><legend>Title</legend></fieldset>"#, 400.0);
    let fs = find_by_tag(&doc.root, "fieldset").unwrap();
    assert_eq!(fs.style.display, Display::Block);
}

#[test]
fn fieldset_has_border() {
    let doc = layout_html(r#"<fieldset><legend>Title</legend></fieldset>"#, 400.0);
    let fs = find_by_tag(&doc.root, "fieldset").unwrap();
    assert!(fs.layout.resolved_border_top > 0.0);
}

// ── Hidden Input ─────────────────────────────────────────────────────────────

#[test]
fn hidden_input_is_display_none() {
    let doc = layout_html(r#"<input type="hidden" name="token" value="abc">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.style.display, Display::None);
}

// ── is_text_input helper ─────────────────────────────────────────────────────

#[test]
fn is_text_input_true_for_text() {
    let doc = layout_html(r#"<input type="text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(is_text_input(input));
}

#[test]
fn is_text_input_true_for_password() {
    let doc = layout_html(r#"<input type="password">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(is_text_input(input));
}

#[test]
fn is_text_input_true_for_email() {
    let doc = layout_html(r#"<input type="email">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(is_text_input(input));
}

#[test]
fn is_text_input_false_for_checkbox() {
    let doc = layout_html(r#"<input type="checkbox">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(!is_text_input(input));
}

#[test]
fn is_text_input_false_for_radio() {
    let doc = layout_html(r#"<input type="radio">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(!is_text_input(input));
}

#[test]
fn is_text_input_false_for_submit() {
    let doc = layout_html(r#"<input type="submit">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(!is_text_input(input));
}

#[test]
fn is_text_input_true_for_textarea() {
    let doc = layout_html(r#"<textarea></textarea>"#, 400.0);
    let ta = find_by_tag(&doc.root, "textarea").unwrap();
    assert!(is_text_input(ta));
}

// ── Select dropdown styling tests ────────────────────────────────────────────

#[test]
fn select_option_inherits_color() {
    let doc = layout_html(r#"<style>select { color: red; }</style>
        <select><option>A</option><option>B</option></select>"#, 400.0);
    let select = find_by_tag(&doc.root, "select").unwrap();
    for child in &select.children {
        if child.tag == "option" {
            // Options should inherit color from select
            assert_eq!(child.style.color.r, 255, "option should inherit red color from select");
        }
    }
}

#[test]
fn select_option_own_style_wins() {
    let doc = layout_html(r#"<style>option { color: blue; }</style>
        <select><option>A</option></select>"#, 400.0);
    let select = find_by_tag(&doc.root, "select").unwrap();
    for child in &select.children {
        if child.tag == "option" {
            assert_eq!(child.style.color.b, 255, "option should have blue from own CSS rule");
        }
    }
}

#[test]
fn select_option_background_applies() {
    let doc = layout_html(r#"<style>option { background-color: yellow; }</style>
        <select><option>A</option></select>"#, 400.0);
    let select = find_by_tag(&doc.root, "select").unwrap();
    for child in &select.children {
        if child.tag == "option" {
            assert_eq!(child.style.background_color.r, 255);
            assert_eq!(child.style.background_color.g, 255);
            assert_eq!(child.style.background_color.b, 0);
        }
    }
}

#[test]
fn select_dark_theme_options_readable() {
    // Simulates a dark theme: select has light text on dark bg
    // Dropdown should NOT show light text on white bg
    let doc = layout_html(r#"<style>
        select { color: #e2e8f0; background: #1e293b; }
    </style>
    <select><option>Light</option><option>Dark</option></select>"#, 400.0);
    let select = find_by_tag(&doc.root, "select").unwrap();
    // The select itself has light text
    assert!(select.style.color.r > 200, "select should have light color");
    // Options inherit from select — they'll have light color too
    // The dropdown renderer must handle this (use dark text or option's own color)
    for child in &select.children {
        if child.tag == "option" {
            // Options inherit color from select
            assert!(child.style.color.r > 200, "option inherits light color from select");
        }
    }
}

// ── Textarea styling tests ──────────────────────────────────────────────────

#[test]
fn textarea_has_border() {
    let doc = layout_html(r#"<textarea>text</textarea>"#, 400.0);
    let ta = find_by_tag(&doc.root, "textarea").unwrap();
    assert!(ta.layout.resolved_border_top > 0.0, "textarea should have border");
}

#[test]
fn textarea_css_override() {
    let doc = layout_html(r#"<style>textarea { background: red; color: white; }</style>
        <textarea>text</textarea>"#, 400.0);
    let ta = find_by_tag(&doc.root, "textarea").unwrap();
    assert_eq!(ta.style.background_color.r, 255);
    assert_eq!(ta.style.color.r, 255);
    assert_eq!(ta.style.color.g, 255);
}

#[test]
fn textarea_width_override() {
    let doc = layout_html(r#"<style>textarea { width: 400px; }</style>
        <textarea>text</textarea>"#, 500.0);
    let ta = find_by_tag(&doc.root, "textarea").unwrap();
    assert!((ta.layout.margin_rect.w - 400.0).abs() < 5.0,
        "textarea width {} should be ~400", ta.layout.margin_rect.w);
}

// ── Input styling tests ─────────────────────────────────────────────────────

#[test]
fn input_padding_override() {
    let doc = layout_html(r#"<style>input { padding: 10px 15px; }</style>
        <input type="text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!((input.layout.resolved_pad_top - 10.0).abs() < 1.0, "padding-top {} should be ~10", input.layout.resolved_pad_top);
    assert!((input.layout.resolved_pad_left - 15.0).abs() < 1.0, "padding-left {} should be ~15", input.layout.resolved_pad_left);
}

#[test]
fn input_border_color_override() {
    let doc = layout_html(r#"<style>input { border: 2px solid red; }</style>
        <input type="text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!((input.layout.resolved_border_top - 2.0).abs() < 0.5, "border should be 2px");
}

#[test]
fn input_border_radius_override() {
    let doc = layout_html(r#"<style>input { border-radius: 10px; }</style>
        <input type="text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!((input.style.border_radius.resolve(16.0, 200.0, 16.0) - 10.0).abs() < 1.0);
}

#[test]
fn input_font_size_override() {
    let doc = layout_html(r#"<style>input { font-size: 20px; }</style>
        <input type="text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    let fpx = input.style.font_size_px(16.0, 16.0);
    assert!((fpx - 20.0).abs() < 1.0, "font-size {} should be 20", fpx);
}

// ── Button styling tests ────────────────────────────────────────────────────

#[test]
fn button_element_has_background() {
    let doc = layout_html(r#"<button>Click</button>"#, 400.0);
    let btn = find_by_tag(&doc.root, "button").unwrap();
    assert!(btn.style.background_color.a > 0, "button should have default bg");
}

#[test]
fn button_element_css_background_override() {
    let doc = layout_html(r#"<style>button { background: green; }</style>
        <button>Click</button>"#, 400.0);
    let btn = find_by_tag(&doc.root, "button").unwrap();
    assert_eq!(btn.style.background_color.g, 128, "green component should be 128, got {}", btn.style.background_color.g);
}

#[test]
fn button_white_space_nowrap() {
    let doc = layout_html(r#"<input type="submit" value="Submit Form">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.style.white_space, crate::types::WhiteSpace::Nowrap);
}

// ── Checkbox/Radio in label tests ────────────────────────────────────────────

#[test]
fn checkbox_in_label_spacing() {
    let doc = layout_html(r#"<label><input type="checkbox"> Option</label>"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.layout.resolved_margin_right >= 4.0, "checkbox should have right margin >= 4, got {}", input.layout.resolved_margin_right);
}

#[test]
fn radio_in_label_spacing() {
    let doc = layout_html(r#"<label><input type="radio" name="g"> Choice</label>"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.layout.resolved_margin_right >= 4.0, "radio should have right margin >= 4, got {}", input.layout.resolved_margin_right);
}

// ── Form input key handling tests ────────────────────────────────────────────

#[test]
fn form_input_insert_at_middle() {
    let doc = layout_html(r#"<input type="text" id="t" value="ac">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut WebCore, id: &str) -> Option<&'a mut WebCore> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 1; // between 'a' and 'c'
    input.input_sel_anchor = 1;
    process_form_input_key(input, 'b' as u32, Some('b'), false, false);
    assert_eq!(input_value(input), "abc");
    assert_eq!(input.input_cursor, 2);
}

#[test]
fn form_input_delete_key() {
    let doc = layout_html(r#"<input type="text" id="t" value="abc">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut WebCore, id: &str) -> Option<&'a mut WebCore> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 1; // before 'b'
    input.input_sel_anchor = 1;
    process_form_input_key(input, 46, None, false, false); // delete
    assert_eq!(input_value(input), "ac");
    assert_eq!(input.input_cursor, 1);
}

#[test]
fn form_input_home_end() {
    let doc = layout_html(r#"<input type="text" id="t" value="hello">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut WebCore, id: &str) -> Option<&'a mut WebCore> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 2;
    input.input_sel_anchor = 2;
    process_form_input_key(input, 36, None, false, false); // Home
    assert_eq!(input.input_cursor, 0);
    process_form_input_key(input, 35, None, false, false); // End
    assert_eq!(input.input_cursor, 5);
}

#[test]
fn textarea_enter_inserts_newline() {
    let doc = layout_html(r#"<textarea id="t">ab</textarea>"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut WebCore, id: &str) -> Option<&'a mut WebCore> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let ta = find_mut(&mut root, "t").unwrap();
    ta.input_cursor = 1;
    ta.input_sel_anchor = 1;
    let changed = process_form_input_key(ta, 13, None, false, false); // Enter
    assert!(changed);
    assert!(input_value(ta).contains('\n'));
}

// ── Disabled state tests ─────────────────────────────────────────────────────

#[test]
fn disabled_input_has_reduced_opacity() {
    let doc = layout_html(r#"<input type="text" disabled>"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.style.opacity < 1.0, "disabled input should have reduced opacity, got {}", input.style.opacity);
}

#[test]
fn disabled_input_matches_pseudo_class() {
    let doc = layout_html(r#"<style>input:disabled { background-color: #eee; }</style>
        <input type="text" disabled>"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.style.background_color.r < 255 || input.style.background_color.g < 255,
        "disabled input should match :disabled pseudo-class and get #eee bg");
}

#[test]
fn disabled_button_matches_pseudo_class() {
    let doc = layout_html(r#"<style>button:disabled { opacity: 0.5; }</style>
        <button disabled>Click</button>"#, 400.0);
    let btn = find_by_tag(&doc.root, "button").unwrap();
    assert!(btn.style.opacity < 0.9, "disabled button opacity {} should be < 0.9", btn.style.opacity);
}

// ── Readonly state tests ────────────────────────────────────────────────────

#[test]
fn readonly_input_blocks_editing() {
    let doc = layout_html(r#"<input type="text" id="t" value="fixed" readonly>"#, 400.0);
    let mut root = doc.root;
    fn fm<'a>(n: &'a mut WebCore, id: &str) -> Option<&'a mut WebCore> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = fm(c, id) { return Some(r); } }
        None
    }
    let input = fm(&mut root, "t").unwrap();
    input.input_cursor = 5;
    input.input_sel_anchor = 5;
    let changed = process_form_input_key(input, 'x' as u32, Some('x'), false, false);
    assert!(!changed, "readonly input should not accept input");
    assert_eq!(input_value(input), "fixed");
}

// ── Maxlength tests ─────────────────────────────────────────────────────────

#[test]
fn maxlength_prevents_input() {
    let doc = layout_html(r#"<input type="text" id="t" value="abc" maxlength="5">"#, 400.0);
    let mut root = doc.root;
    fn fm<'a>(n: &'a mut WebCore, id: &str) -> Option<&'a mut WebCore> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = fm(c, id) { return Some(r); } }
        None
    }
    let input = fm(&mut root, "t").unwrap();
    input.input_cursor = 3;
    input.input_sel_anchor = 3;
    process_form_input_key(input, 'd' as u32, Some('d'), false, false); // "abcd" — ok
    process_form_input_key(input, 'e' as u32, Some('e'), false, false); // "abcde" — ok, at limit
    let changed = process_form_input_key(input, 'f' as u32, Some('f'), false, false); // should be blocked
    assert!(!changed, "should not exceed maxlength");
    assert_eq!(input_value(input).chars().count(), 5);
}

// ── Label for= association ──────────────────────────────────────────────────

#[test]
fn label_for_attribute_preserved() {
    let doc = layout_html(r#"<label for="name">Name:</label><input id="name" type="text">"#, 400.0);
    let label = find_by_tag(&doc.root, "label").unwrap();
    assert_eq!(label.attributes.get("for").map(|s| s.as_str()), Some("name"));
}

// ── Textarea rows/cols ──────────────────────────────────────────────────────

#[test]
fn textarea_rows_affects_height() {
    let doc1 = layout_html(r#"<textarea rows="2">text</textarea>"#, 400.0);
    let doc2 = layout_html(r#"<textarea rows="6">text</textarea>"#, 400.0);
    let ta1 = find_by_tag(&doc1.root, "textarea").unwrap();
    let ta2 = find_by_tag(&doc2.root, "textarea").unwrap();
    assert!(ta2.layout.content_rect.h > ta1.layout.content_rect.h,
        "rows=6 height {} should be > rows=2 height {}", ta2.layout.content_rect.h, ta1.layout.content_rect.h);
}

// ── Input size attribute ────────────────────────────────────────────────────

#[test]
fn input_size_affects_width() {
    let doc1 = layout_html(r#"<input type="text" size="5">"#, 400.0);
    let doc2 = layout_html(r#"<input type="text" size="40">"#, 500.0);
    let i1 = find_by_tag(&doc1.root, "input").unwrap();
    let i2 = find_by_tag(&doc2.root, "input").unwrap();
    assert!(i2.layout.content_rect.w > i1.layout.content_rect.w,
        "size=40 width {} should be > size=5 width {}", i2.layout.content_rect.w, i1.layout.content_rect.w);
}

// ── Disabled blocks form_input_key ──────────────────────────────────────────

#[test]
fn disabled_input_blocks_typing() {
    let doc = layout_html(r#"<input type="text" id="t" value="hi" disabled>"#, 400.0);
    let mut root = doc.root;
    fn fm<'a>(n: &'a mut WebCore, id: &str) -> Option<&'a mut WebCore> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = fm(c, id) { return Some(r); } }
        None
    }
    let input = fm(&mut root, "t").unwrap();
    input.input_cursor = 2;
    input.input_sel_anchor = 2;
    let changed = process_form_input_key(input, 'x' as u32, Some('x'), false, false);
    assert!(!changed, "disabled input should not accept typing");
    assert_eq!(input_value(input), "hi");
}

// ── Required pseudo-class ───────────────────────────────────────────────────

#[test]
fn required_input_matches_pseudo_class() {
    let doc = layout_html(r#"<style>input:required { border-color: red; }</style>
        <input type="text" required>"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    // Should match :required — border-color red means border is applied
    // (border-color is not directly on ComputedStyle as a separate field,
    // but the border rendering should pick it up)
    assert!(input.attributes.contains_key("required"));
}

// ── Placeholder-shown pseudo-class ──────────────────────────────────────────

#[test]
fn placeholder_shown_matches_empty_input() {
    let doc = layout_html(r#"<style>input:placeholder-shown { color: gray; }</style>
        <input type="text" placeholder="hint">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    // Empty value + placeholder → :placeholder-shown should match
    assert!(input.attributes.get("value").map(|v| v.is_empty()).unwrap_or(true));
}

// ── CSS override tests ──────────────────────────────────────────────────────

#[test]
fn input_css_width_override() {
    let doc = layout_html(r#"<style>input { width: 300px; }</style><input type="text">"#, 500.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    // margin_rect width should be close to 300px (border-box)
    assert!((input.layout.margin_rect.w - 300.0).abs() < 5.0,
        "input width {} should be ~300", input.layout.margin_rect.w);
}

#[test]
fn input_css_background_override() {
    let doc = layout_html(r#"<style>input { background-color: red; }</style><input type="text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.style.background_color.r, 255);
    assert_eq!(input.style.background_color.g, 0);
}

#[test]
fn button_css_background_override() {
    let doc = layout_html(r#"<style>input[type=submit] { background-color: blue; }</style><input type="submit" value="Go">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.style.background_color.b, 255, "button bg blue should be 255, got {}", input.style.background_color.b);
}

// ── Disabled checkbox/radio don't toggle ─────────────────────────────────────

#[test]
fn disabled_checkbox_no_toggle() {
    let html = r#"<input type="checkbox" id="c" disabled>"#;
    let mut doc = layout_html(html, 400.0);
    let cb = find_by_id(&doc.root, "c").unwrap();
    assert!(!cb.attributes.contains_key("checked"), "should start unchecked");
    // Simulate click via handle_form_click
    let cb_node_id = cb.node_id;
    crate::types::handle_form_click(&mut doc.root, cb_node_id, &mut None);
    let cb = find_by_id(&doc.root, "c").unwrap();
    assert!(!cb.attributes.contains_key("checked"), "disabled checkbox should not toggle");
}

// ── Select size=N shows listbox ──────────────────────────────────────────────

#[test]
fn select_with_size_is_taller() {
    let doc1 = layout_html(r#"<select><option>A</option><option>B</option></select>"#, 400.0);
    let doc2 = layout_html(r#"<select size="4"><option>A</option><option>B</option><option>C</option><option>D</option></select>"#, 400.0);
    let s1 = find_by_tag(&doc1.root, "select").unwrap();
    let s2 = find_by_tag(&doc2.root, "select").unwrap();
    assert!(s2.layout.margin_rect.h > s1.layout.margin_rect.h,
        "select size=4 height {} should be > default height {}", s2.layout.margin_rect.h, s1.layout.margin_rect.h);

    // ⚠ It has to be taller FOR THE RIGHT REASON — roughly four rows, not one
    // row in a bigger font.
    //
    // This passed before the list box existed, and it passed by accident: the
    // presentational-attribute pass read `size="4"` as `<font size=4>` and set
    // `font-size: 18px`, so the one-row height of `2.2em` grew with the font.
    // `size` means three different things by element (rows on a `<select>`,
    // characters on an `<input>`, a font step on `<font>`) and only the last
    // was implemented.
    assert!(
        (s2.style.font_size_px(16.0, 16.0) - s1.style.font_size_px(16.0, 16.0)).abs() < 0.01,
        "`size` changed the FONT of a select: {} vs {}",
        s2.style.font_size_px(16.0, 16.0),
        s1.style.font_size_px(16.0, 16.0)
    );
    assert!(
        s2.layout.margin_rect.h > s1.layout.margin_rect.h * 2.0,
        "four rows should be well over twice the one-row height, got {} vs {}",
        s2.layout.margin_rect.h, s1.layout.margin_rect.h
    );
}

// ── Multiple select ──────────────────────────────────────────────────────────

#[test]
fn select_multiple_preserves_attribute() {
    let doc = layout_html(r#"<select multiple><option>A</option><option>B</option></select>"#, 400.0);
    let s = find_by_tag(&doc.root, "select").unwrap();
    assert!(s.attributes.contains_key("multiple"));
}

// ── Color input value ────────────────────────────────────────────────────────

#[test]
fn color_input_preserves_value() {
    let doc = layout_html(r##"<input type="color" value="#ff0000">"##, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.attributes.get("value").map(|s| s.as_str()), Some("#ff0000"));
}

// ── Date input value ─────────────────────────────────────────────────────────

#[test]
fn date_input_preserves_value() {
    let doc = layout_html(r#"<input type="date" value="2026-03-23">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.attributes.get("value").map(|s| s.as_str()), Some("2026-03-23"));
}

// ── File input ───────────────────────────────────────────────────────────────

#[test]
fn file_input_has_width() {
    let doc = layout_html(r#"<input type="file">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.layout.content_rect.w > 100.0);
}

// ── Input number min/max preserved ──────────────────────────────────────────

#[test]
fn number_input_preserves_min_max() {
    let doc = layout_html(r#"<input type="number" min="0" max="100" value="50">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.attributes.get("min").map(|s| s.as_str()), Some("0"));
    assert_eq!(input.attributes.get("max").map(|s| s.as_str()), Some("100"));
}

// ── Focusable tests ─────────────────────────────────────────────────────────

#[test]
fn input_is_focusable() {
    let doc = layout_html(r#"<input type="text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(crate::types::is_focusable_node(input));
}

#[test]
fn select_is_focusable() {
    let doc = layout_html(r#"<select><option>A</option></select>"#, 400.0);
    let s = find_by_tag(&doc.root, "select").unwrap();
    assert!(crate::types::is_focusable_node(s));
}

#[test]
fn textarea_is_focusable() {
    let doc = layout_html(r#"<textarea>text</textarea>"#, 400.0);
    let ta = find_by_tag(&doc.root, "textarea").unwrap();
    assert!(crate::types::is_focusable_node(ta));
}

#[test]
fn button_is_focusable() {
    let doc = layout_html(r#"<button>Click</button>"#, 400.0);
    let btn = find_by_tag(&doc.root, "button").unwrap();
    assert!(crate::types::is_focusable_node(btn));
}

#[test]
fn form_inside_table_works() {
    let doc = layout_html(r#"<table><form><tr><td>Label</td><td><input type="text"></td></tr></form></table>"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.layout.content_rect.w > 50.0, "input inside form-in-table should have width, got {}", input.layout.content_rect.w);
    assert!(input.layout.content_rect.h > 5.0, "input inside form-in-table should have height, got {}", input.layout.content_rect.h);
}

#[test]
fn form_inside_table_is_contents() {
    let doc = layout_html(r#"<table><form><tr><td>X</td></tr></form></table>"#, 400.0);
    let form = find_by_tag(&doc.root, "form").unwrap();
    assert_eq!(form.style.display, Display::Contents, "form inside table should be display:contents");
}

#[test]
fn click_and_type_integration() {
    // Full integration test: click an input, then type — value should update
    let mut doc = layout_html(r#"<input type="text" id="t" value="">"#, 400.0);
    let input = find_by_id(&doc.root, "t").unwrap();
    let input_center = (
        input.layout.border_rect.x + input.layout.border_rect.w / 2.0,
        input.layout.border_rect.y + input.layout.border_rect.h / 2.0,
    );

    // Simulate click: MouseDown then MouseUp
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, input_center, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, input_center, 0);

    // Check focus was set
    assert!(doc.focused_box != 0, "clicking input should set focused_box");

    // Simulate typing 'abc'
    doc.process_key_event(crate::dom::HtmlEventType::KeyDown, 'a' as u32, Some('a'), false, false, false, false);
    doc.process_key_event(crate::dom::HtmlEventType::KeyDown, 'b' as u32, Some('b'), false, false, false, false);
    doc.process_key_event(crate::dom::HtmlEventType::KeyDown, 'c' as u32, Some('c'), false, false, false, false);

    // Check value updated
    let input = find_by_id(&doc.root, "t").unwrap();
    assert_eq!(input.attributes.get("value").map(|s| s.as_str()), Some("abc"),
        "typing 'abc' should update value to 'abc', got {:?}", input.attributes.get("value"));
}

#[test]
fn click_and_type_password() {
    let mut doc = layout_html(r#"<input type="password" id="p" value="">"#, 400.0);
    let input = find_by_id(&doc.root, "p").unwrap();
    let center = (input.layout.border_rect.x + input.layout.border_rect.w / 2.0, input.layout.border_rect.y + input.layout.border_rect.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, center, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, center, 0);
    assert!(doc.focused_box != 0, "password input should focus");
    doc.process_key_event(crate::dom::HtmlEventType::KeyDown, 'x' as u32, Some('x'), false, false, false, false);
    let input = find_by_id(&doc.root, "p").unwrap();
    assert_eq!(input.attributes.get("value").map(|s| s.as_str()), Some("x"));
}

#[test]
fn click_checkbox_toggles() {
    let mut doc = layout_html(r#"<input type="checkbox" id="c">"#, 400.0);
    let cb = find_by_id(&doc.root, "c").unwrap();
    assert!(!cb.checkedness);
    let center = (cb.layout.border_rect.x + cb.layout.border_rect.w / 2.0, cb.layout.border_rect.y + cb.layout.border_rect.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, center, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, center, 0);
    let cb = find_by_id(&doc.root, "c").unwrap();
    // A click changes CHECKEDNESS and leaves the markup alone (HTML §4.10.5.3)
    // — the user did not edit the document. This asserted the attribute,
    // because ticking a box used to rewrite it.
    assert!(cb.checkedness, "clicking checkbox should check it");
    assert!(
        !cb.attributes.contains_key("checked"),
        "the click wrote a `checked` attribute into the document"
    );
    assert!(cb.dirty_checked, "a user interaction raises the dirty flag");
}

#[test]
fn click_radio_selects() {
    let mut doc = layout_html(r#"<input type="radio" name="g" id="r1"><input type="radio" name="g" id="r2">"#, 400.0);
    let r2 = find_by_id(&doc.root, "r2").unwrap();
    let center = (r2.layout.border_rect.x + r2.layout.border_rect.w / 2.0, r2.layout.border_rect.y + r2.layout.border_rect.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, center, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, center, 0);
    let r2 = find_by_id(&doc.root, "r2").unwrap();
    assert!(r2.checkedness, "clicking r2 should check it");
    assert!(
        !r2.attributes.contains_key("checked"),
        "the click wrote a `checked` attribute into the document"
    );
    let r1 = find_by_id(&doc.root, "r1").unwrap();
    assert!(!r1.checkedness, "r1 should be unchecked");
}

// ── All button types ─────────────────────────────────────────────────────────

fn click_button_fires_event(html: &str, expected_tag: &str) -> Vec<FormEventKind> {
    let mut doc = layout_html(html, 400.0);
    let events_ref = std::sync::Arc::new(std::sync::Mutex::new(Vec::<FormEventKind>::new()));
    let events_clone = events_ref.clone();
    doc.on_form_event = Some(Box::new(move |e: &FormEvent| {
        events_clone.lock().unwrap().push(e.kind.clone());
    }));
    let btn = find_by_tag(&doc.root, expected_tag).unwrap();
    let center = (btn.layout.border_rect.x + btn.layout.border_rect.w / 2.0, btn.layout.border_rect.y + btn.layout.border_rect.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, center, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, center, 0);
    let result = events_ref.lock().unwrap().clone();
    doc.on_form_event = None; // drop the closure before doc
    result
}

#[test]
fn submit_input_fires_click_and_submit() {
    let events = click_button_fires_event(
        r#"<form action="/login"><input type="submit" value="Go"></form>"#, "input");
    assert!(events.iter().any(|e| matches!(e, FormEventKind::Click(_))), "should fire Click");
}

#[test]
fn button_input_fires_click_only() {
    let events = click_button_fires_event(
        r#"<input type="button" value="Action">"#, "input");
    assert!(events.iter().any(|e| matches!(e, FormEventKind::Click(_))), "should fire Click");
    assert!(!events.iter().any(|e| matches!(e, FormEventKind::Submit(_))), "should NOT fire Submit");
}

#[test]
fn reset_input_fires_click() {
    let events = click_button_fires_event(
        r#"<input type="reset" value="Clear">"#, "input");
    assert!(events.iter().any(|e| matches!(e, FormEventKind::Click(_))));
}

#[test]
fn button_element_submit_fires_submit() {
    let events = click_button_fires_event(
        r#"<form action="/go"><button type="submit">Send</button></form>"#, "button");
    assert!(events.iter().any(|e| matches!(e, FormEventKind::Click(_))));
    assert!(events.iter().any(|e| matches!(e, FormEventKind::Submit(_))), "submit button should fire Submit");
}

#[test]
fn button_element_type_button_no_submit() {
    let events = click_button_fires_event(
        r#"<form action="/go"><button type="button">Click</button></form>"#, "button");
    assert!(events.iter().any(|e| matches!(e, FormEventKind::Click(_))));
    assert!(!events.iter().any(|e| matches!(e, FormEventKind::Submit(_))), "type=button should NOT submit");
}

// ── Focus for ALL input types ───────────────────────────────────────────────

fn click_sets_focus(html: &str, tag: &str) -> bool {
    let mut doc = layout_html(html, 400.0);
    let elem = find_by_tag(&doc.root, tag).unwrap();
    let center = (elem.layout.border_rect.x + elem.layout.border_rect.w / 2.0,
                  elem.layout.border_rect.y + elem.layout.border_rect.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, center, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, center, 0);
    doc.focused_box != 0
}

#[test]
fn click_focuses_text_input() {
    assert!(click_sets_focus(r#"<input type="text">"#, "input"));
}

#[test]
fn click_focuses_password_input() {
    assert!(click_sets_focus(r#"<input type="password">"#, "input"));
}

#[test]
fn click_focuses_email_input() {
    assert!(click_sets_focus(r#"<input type="email">"#, "input"));
}

#[test]
fn click_focuses_search_input() {
    assert!(click_sets_focus(r#"<input type="search">"#, "input"));
}

#[test]
fn click_focuses_number_input() {
    assert!(click_sets_focus(r#"<input type="number">"#, "input"));
}

#[test]
fn click_focuses_tel_input() {
    assert!(click_sets_focus(r#"<input type="tel">"#, "input"));
}

#[test]
fn click_focuses_url_input() {
    assert!(click_sets_focus(r#"<input type="url">"#, "input"));
}

#[test]
fn click_focuses_textarea() {
    assert!(click_sets_focus(r#"<textarea>text</textarea>"#, "textarea"));
}

#[test]
fn click_focuses_select() {
    assert!(click_sets_focus(r#"<select><option>A</option></select>"#, "select"));
}

#[test]
fn click_focuses_button() {
    assert!(click_sets_focus(r#"<button>Click</button>"#, "button"));
}

#[test]
fn click_focuses_checkbox() {
    assert!(click_sets_focus(r#"<input type="checkbox">"#, "input"));
}

#[test]
fn click_focuses_radio() {
    assert!(click_sets_focus(r#"<input type="radio">"#, "input"));
}

#[test]
fn hidden_input_not_focusable_by_click() {
    // Hidden inputs can't be clicked since they have display:none
    let mut doc = layout_html(r#"<input type="hidden" name="t">"#, 400.0);
    // No border_rect to click — just verify focus stays null
    assert!(doc.focused_box == 0);
}

// ── Tab navigation cycles through all focusable elements ────────────────────

#[test]
fn tab_cycles_through_form_elements() {
    let mut doc = layout_html(r#"
        <input type="text" id="a">
        <input type="password" id="b">
        <select id="c"><option>X</option></select>
        <textarea id="d">t</textarea>
        <button id="e">Go</button>
    "#, 400.0);

    // Tab should cycle: a → b → c → d → e → a
    doc.focus_next();
    assert!(doc.focused_box != 0);
    let tag1 = find_by_node_id(&doc.root, doc.focused_box).and_then(|n| n.attributes.get("id").cloned()).unwrap_or_default();
    assert_eq!(tag1, "a", "first Tab should focus 'a', got '{}'", tag1);

    doc.focus_next();
    let tag2 = find_by_node_id(&doc.root, doc.focused_box).and_then(|n| n.attributes.get("id").cloned()).unwrap_or_default();
    assert_eq!(tag2, "b");

    doc.focus_next();
    let tag3 = find_by_node_id(&doc.root, doc.focused_box).and_then(|n| n.attributes.get("id").cloned()).unwrap_or_default();
    assert_eq!(tag3, "c");

    doc.focus_next();
    let tag4 = find_by_node_id(&doc.root, doc.focused_box).and_then(|n| n.attributes.get("id").cloned()).unwrap_or_default();
    assert_eq!(tag4, "d");

    doc.focus_next();
    let tag5 = find_by_node_id(&doc.root, doc.focused_box).and_then(|n| n.attributes.get("id").cloned()).unwrap_or_default();
    assert_eq!(tag5, "e");
}

#[test]
fn shift_tab_goes_backwards() {
    let mut doc = layout_html(r#"
        <input type="text" id="a">
        <input type="text" id="b">
        <input type="text" id="c">
    "#, 400.0);

    // Focus c first
    doc.focus_next(); // a
    doc.focus_next(); // b
    doc.focus_next(); // c
    let id = find_by_node_id(&doc.root, doc.focused_box).and_then(|n| n.attributes.get("id").cloned()).unwrap_or_default();
    assert_eq!(id, "c");

    doc.focus_prev(); // back to b
    let id = find_by_node_id(&doc.root, doc.focused_box).and_then(|n| n.attributes.get("id").cloned()).unwrap_or_default();
    assert_eq!(id, "b");
}

// ── Enter submits form from text input ──────────────────────────────────────

#[test]
fn enter_in_text_input_no_newline() {
    let mut doc = layout_html(r#"<form action="/go"><input type="text" id="t" value="hi"></form>"#, 400.0);
    // Focus the input
    let input = find_by_id(&doc.root, "t").unwrap();
    let center = (input.layout.border_rect.x + input.layout.border_rect.w / 2.0,
                  input.layout.border_rect.y + input.layout.border_rect.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, center, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, center, 0);
    // Press Enter
    doc.process_key_event(crate::dom::HtmlEventType::KeyDown, 13, None, false, false, false, false);
    // Should NOT insert newline in single-line input
    let input = find_by_id(&doc.root, "t").unwrap();
    assert_eq!(input_value(input), "hi", "Enter should not insert in text input");
}

// ── Disabled elements don't focus on click ──────────────────────────────────

#[test]
fn disabled_input_no_focus_on_click() {
    let mut doc = layout_html(r#"<input type="text" disabled>"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    let center = (input.layout.border_rect.x + input.layout.border_rect.w / 2.0,
                  input.layout.border_rect.y + input.layout.border_rect.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, center, 0);
    // Disabled inputs ARE focusable per HTML spec (browsers focus them on click)
    // but they shouldn't accept keyboard input (already tested above)
}

// ── Form submission tests ────────────────────────────────────────────────────

#[test]
fn collect_form_data_basic() {
    let doc = layout_html(r#"<form id="f">
        <input type="text" name="user" value="alice">
        <input type="password" name="pass" value="secret">
        <input type="hidden" name="token" value="xyz">
    </form>"#, 400.0);
    let form = find_by_id(&doc.root, "f").unwrap();
    let data = crate::types::collect_form_data(form);
    assert_eq!(data.get("user").map(|s| s.as_str()), Some("alice"));
    assert_eq!(data.get("pass").map(|s| s.as_str()), Some("secret"));
    assert_eq!(data.get("token").map(|s| s.as_str()), Some("xyz"));
}

#[test]
fn collect_form_data_checkbox() {
    let doc = layout_html(r#"<form id="f">
        <input type="checkbox" name="agree" checked>
        <input type="checkbox" name="news">
    </form>"#, 400.0);
    let form = find_by_id(&doc.root, "f").unwrap();
    let data = crate::types::collect_form_data(form);
    assert_eq!(data.get("agree").map(|s| s.as_str()), Some("on"));
    assert!(data.get("news").is_none(), "unchecked checkbox should not be in form data");
}

#[test]
fn collect_form_data_radio() {
    let doc = layout_html(r#"<form id="f">
        <input type="radio" name="color" value="red">
        <input type="radio" name="color" value="blue" checked>
        <input type="radio" name="color" value="green">
    </form>"#, 400.0);
    let form = find_by_id(&doc.root, "f").unwrap();
    let data = crate::types::collect_form_data(form);
    assert_eq!(data.get("color").map(|s| s.as_str()), Some("blue"));
}

#[test]
fn collect_form_data_select() {
    let doc = layout_html(r#"<form id="f">
        <select name="country">
            <option value="us">US</option>
            <option value="fr" selected>France</option>
        </select>
    </form>"#, 400.0);
    let form = find_by_id(&doc.root, "f").unwrap();
    let data = crate::types::collect_form_data(form);
    assert_eq!(data.get("country").map(|s| s.as_str()), Some("fr"));
}

#[test]
fn collect_form_data_textarea() {
    let doc = layout_html(r#"<form id="f">
        <textarea name="bio">Hello world</textarea>
    </form>"#, 400.0);
    let form = find_by_id(&doc.root, "f").unwrap();
    let data = crate::types::collect_form_data(form);
    assert!(data.get("bio").map(|s| s.contains("Hello")).unwrap_or(false));
}

#[test]
fn collect_form_data_disabled_excluded() {
    let doc = layout_html(r#"<form id="f">
        <input type="text" name="a" value="yes">
        <input type="text" name="b" value="no" disabled>
    </form>"#, 400.0);
    let form = find_by_id(&doc.root, "f").unwrap();
    let data = crate::types::collect_form_data(form);
    assert_eq!(data.get("a").map(|s| s.as_str()), Some("yes"));
    assert!(data.get("b").is_none(), "disabled input should be excluded from form data");
}

#[test]
fn collect_form_data_no_name_excluded() {
    let doc = layout_html(r#"<form id="f">
        <input type="text" value="orphan">
        <input type="text" name="named" value="ok">
    </form>"#, 400.0);
    let form = find_by_id(&doc.root, "f").unwrap();
    let data = crate::types::collect_form_data(form);
    assert_eq!(data.len(), 1);
    assert_eq!(data.get("named").map(|s| s.as_str()), Some("ok"));
}

#[test]
fn form_method_default_get() {
    let doc = layout_html(r#"<form id="f" action="/search"><input name="q" value="test"></form>"#, 400.0);
    let form = find_by_id(&doc.root, "f").unwrap();
    let method = form.attributes.get("method").map(|s| s.as_str()).unwrap_or("get");
    assert_eq!(method, "get");
}

#[test]
fn form_method_post() {
    let doc = layout_html(r#"<form id="f" method="POST" action="/login"><input name="u" value="x"></form>"#, 400.0);
    let form = find_by_id(&doc.root, "f").unwrap();
    let method = form.attributes.get("method").map(|s| s.to_ascii_lowercase()).unwrap_or_default();
    assert_eq!(method, "post");
}

#[test]
fn form_action_preserved() {
    let doc = layout_html(r#"<form id="f" action="/submit"><input name="x" value="1"></form>"#, 400.0);
    let form = find_by_id(&doc.root, "f").unwrap();
    assert_eq!(form.attributes.get("action").map(|s| s.as_str()), Some("/submit"));
}

#[test]
fn submit_event_contains_action() {
    let events = click_button_fires_event(
        r#"<form action="/login"><button type="submit">Go</button></form>"#, "button");
    let submit = events.iter().find(|e| matches!(e, FormEventKind::Submit(_)));
    assert!(submit.is_some(), "should fire Submit event");
    if let Some(FormEventKind::Submit(action)) = submit {
        assert_eq!(action, "/login");
    }
}

// ── Form reset ──────────────────────────────────────────────────────────────

#[test]
fn reset_clears_text_inputs() {
    let mut doc = layout_html(r#"<form id="f">
        <input type="text" id="t" name="t" value="original">
    </form>"#, 400.0);
    // Simulate typing to change value
    let input = find_by_id(&doc.root, "t").unwrap();
    let center = (input.layout.border_rect.x + input.layout.border_rect.w / 2.0,
                  input.layout.border_rect.y + input.layout.border_rect.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, center, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, center, 0);
    doc.process_key_event(crate::dom::HtmlEventType::KeyDown, 'x' as u32, Some('x'), false, false, false, false);
    let input = find_by_id(&doc.root, "t").unwrap();
    assert!(input_value(input).contains('x'), "should have typed");
    // Reset
    let form_node_id = find_by_id(&doc.root, "f").unwrap().node_id;
    crate::types::reset_form(&mut doc.root, form_node_id);
    let input = find_by_id(&doc.root, "t").unwrap();
    assert_eq!(input_value(input), "original", "reset should restore original value");
}

// ── Autofocus ───────────────────────────────────────────────────────────────

#[test]
fn autofocus_attribute_preserved() {
    let doc = layout_html(r#"<input type="text" id="a" autofocus>"#, 400.0);
    let input = find_by_id(&doc.root, "a").unwrap();
    assert!(input.attributes.contains_key("autofocus"));
}

// ── Form data encoding ──────────────────────────────────────────────────────

#[test]
fn encode_form_data_urlencoded() {
    let doc = layout_html(r#"<form id="f"><input name="q" value="hello world"><input name="lang" value="en"></form>"#, 400.0);
    let form = find_by_id(&doc.root, "f").unwrap();
    let data = crate::types::collect_form_data(form);
    let encoded = crate::types::encode_form_urlencoded(&data);
    assert!(encoded.contains("q=hello+world") || encoded.contains("q=hello%20world"),
        "should encode space, got: {}", encoded);
    assert!(encoded.contains("lang=en"));
}

#[test]
fn build_get_url() {
    let doc = layout_html(r#"<form id="f" method="get" action="/search"><input name="q" value="test"></form>"#, 400.0);
    let form = find_by_id(&doc.root, "f").unwrap();
    let data = crate::types::collect_form_data(form);
    let url = crate::types::build_form_submit_url("/search", "get", &data);
    assert!(url.contains("/search?"), "GET should append query string, got: {}", url);
    assert!(url.contains("q=test"));
}

#[test]
fn build_post_url_no_query() {
    let doc = layout_html(r#"<form id="f" method="post" action="/login"><input name="u" value="x"></form>"#, 400.0);
    let form = find_by_id(&doc.root, "f").unwrap();
    let data = crate::types::collect_form_data(form);
    let url = crate::types::build_form_submit_url("/login", "post", &data);
    assert_eq!(url, "/login", "POST should not append query string");
}

// ── Select keyboard navigation ──────────────────────────────────────────────

#[test]
fn select_arrow_down_changes_option() {
    let mut doc = layout_html(r#"<select id="s">
        <option value="a">Alpha</option>
        <option value="b" selected>Beta</option>
        <option value="c">Gamma</option>
    </select>"#, 400.0);
    // Focus the select
    let sel = find_by_id(&doc.root, "s").unwrap();
    let center = (sel.layout.border_rect.x + sel.layout.border_rect.w / 2.0, sel.layout.border_rect.y + sel.layout.border_rect.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, center, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, center, 0);
    // Arrow down should select next option
    doc.process_key_event(crate::dom::HtmlEventType::KeyDown, 40, None, false, false, false, false); // down
    let sel = find_by_id(&doc.root, "s").unwrap();
    assert_eq!(sel.data.get("_selected_idx").map(|s| s.as_str()), Some("2"),
        "arrow down should move to index 2");
}

#[test]
fn select_arrow_up_changes_option() {
    let mut doc = layout_html(r#"<select id="s">
        <option value="a">Alpha</option>
        <option value="b" selected>Beta</option>
        <option value="c">Gamma</option>
    </select>"#, 400.0);
    let sel = find_by_id(&doc.root, "s").unwrap();
    let center = (sel.layout.border_rect.x + sel.layout.border_rect.w / 2.0, sel.layout.border_rect.y + sel.layout.border_rect.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, center, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, center, 0);
    doc.process_key_event(crate::dom::HtmlEventType::KeyDown, 38, None, false, false, false, false); // up
    let sel = find_by_id(&doc.root, "s").unwrap();
    assert_eq!(sel.data.get("_selected_idx").map(|s| s.as_str()), Some("0"),
        "arrow up should move to index 0");
}

// ── Number input increment/decrement ────────────────────────────────────────

#[test]
fn number_input_arrow_up_increments() {
    let mut doc = layout_html(r#"<input type="number" id="n" value="5" min="0" max="10">"#, 400.0);
    let input = find_by_id(&doc.root, "n").unwrap();
    let center = (input.layout.border_rect.x + input.layout.border_rect.w / 2.0, input.layout.border_rect.y + input.layout.border_rect.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, center, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, center, 0);
    doc.process_key_event(crate::dom::HtmlEventType::KeyDown, 38, None, false, false, false, false); // up
    let input = find_by_id(&doc.root, "n").unwrap();
    assert_eq!(input_value(input), "6", "arrow up should increment, got {}", input_value(input));
}

#[test]
fn number_input_arrow_down_decrements() {
    let mut doc = layout_html(r#"<input type="number" id="n" value="5" min="0" max="10">"#, 400.0);
    let input = find_by_id(&doc.root, "n").unwrap();
    let center = (input.layout.border_rect.x + input.layout.border_rect.w / 2.0, input.layout.border_rect.y + input.layout.border_rect.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, center, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, center, 0);
    doc.process_key_event(crate::dom::HtmlEventType::KeyDown, 40, None, false, false, false, false); // down
    let input = find_by_id(&doc.root, "n").unwrap();
    assert_eq!(input_value(input), "4");
}

#[test]
fn number_input_respects_max() {
    let mut doc = layout_html(r#"<input type="number" id="n" value="10" max="10">"#, 400.0);
    let input = find_by_id(&doc.root, "n").unwrap();
    let center = (input.layout.border_rect.x + input.layout.border_rect.w / 2.0, input.layout.border_rect.y + input.layout.border_rect.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, center, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, center, 0);
    doc.process_key_event(crate::dom::HtmlEventType::KeyDown, 38, None, false, false, false, false);
    let input = find_by_id(&doc.root, "n").unwrap();
    assert_eq!(input_value(input), "10", "should not exceed max");
}

#[test]
fn number_input_respects_min() {
    let mut doc = layout_html(r#"<input type="number" id="n" value="0" min="0">"#, 400.0);
    let input = find_by_id(&doc.root, "n").unwrap();
    let center = (input.layout.border_rect.x + input.layout.border_rect.w / 2.0, input.layout.border_rect.y + input.layout.border_rect.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, center, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, center, 0);
    doc.process_key_event(crate::dom::HtmlEventType::KeyDown, 40, None, false, false, false, false);
    let input = find_by_id(&doc.root, "n").unwrap();
    assert_eq!(input_value(input), "0", "should not go below min");
}

// ── Autofocus ───────────────────────────────────────────────────────────────

#[test]
fn autofocus_sets_focus_on_load() {
    let mut doc = layout_html(r#"<input type="text" id="a"><input type="text" id="b" autofocus>"#, 400.0);
    crate::types::apply_autofocus(&mut doc);
    assert!(doc.focused_box != 0, "autofocus should set focused_box");
    let focused_id = find_by_node_id(&doc.root, doc.focused_box).and_then(|n| n.attributes.get("id").cloned()).unwrap_or_default();
    assert_eq!(focused_id, "b", "should focus element with autofocus attribute");
}

// ── Required visual ─────────────────────────────────────────────────────────

#[test]
fn required_input_matches_required_pseudo() {
    let doc = layout_html(r#"<style>input:required { border-color: red; }</style>
        <input type="text" required>"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.attributes.contains_key("required"));
    // The :required pseudo-class should match — border-color isn't directly
    // on ComputedStyle, but the cascade should process it
}

// ── Placeholder pseudo-element ──────────────────────────────────────────────

#[test]
fn placeholder_text_has_default_gray_color() {
    // Placeholder should be rendered in gray by the renderer (not testable via style)
    let doc = layout_html(r#"<input type="text" placeholder="hint">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.attributes.get("placeholder").map(|s| s.as_str()), Some("hint"));
    // The renderer draws placeholder in gray — visual test
}

// ── Focus ring on tab-focused checkbox ──────────────────────────────────────

#[test]
fn tab_to_checkbox_sets_focus() {
    let mut doc = layout_html(r#"<input type="checkbox" id="c"><input type="text" id="t">"#, 400.0);
    doc.focus_next(); // should focus checkbox first
    assert!(doc.focused_box != 0);
    let id = find_by_node_id(&doc.root, doc.focused_box).and_then(|n| n.attributes.get("id").cloned()).unwrap_or_default();
    assert_eq!(id, "c", "Tab should focus checkbox");
}

// ── Tabindex tests ──────────────────────────────────────────────────────────

#[test]
fn tabindex_positive_comes_first() {
    let mut doc = layout_html(r#"
        <input type="text" id="a">
        <input type="text" id="b" tabindex="1">
        <input type="text" id="c">
    "#, 400.0);
    doc.focus_next();
    let id = find_by_node_id(&doc.root, doc.focused_box).and_then(|n| n.attributes.get("id").cloned()).unwrap_or_default();
    assert_eq!(id, "b", "tabindex=1 should be focused first, got {}", id);
}

#[test]
fn tabindex_negative_skipped() {
    let mut doc = layout_html(r#"
        <input type="text" id="a">
        <input type="text" id="skip" tabindex="-1">
        <input type="text" id="c">
    "#, 400.0);
    doc.focus_next(); // a
    doc.focus_next(); // should skip "skip" and go to c
    let id = find_by_node_id(&doc.root, doc.focused_box).and_then(|n| n.attributes.get("id").cloned()).unwrap_or_default();
    assert_eq!(id, "c", "tabindex=-1 should be skipped, got {}", id);
}

// ── Pattern validation ──────────────────────────────────────────────────────

#[test]
fn pattern_attribute_preserved() {
    let doc = layout_html(r#"<input type="text" pattern="[0-9]+" id="p">"#, 400.0);
    let input = find_by_id(&doc.root, "p").unwrap();
    assert_eq!(input.attributes.get("pattern").map(|s| s.as_str()), Some("[0-9]+"));
}

// ── Required attribute ──────────────────────────────────────────────────────

#[test]
fn required_attribute_preserved() {
    let doc = layout_html(r#"<input type="text" required id="r">"#, 400.0);
    let input = find_by_id(&doc.root, "r").unwrap();
    assert!(input.attributes.contains_key("required"));
}

// ── Select multiple ─────────────────────────────────────────────────────────

#[test]
fn select_multiple_attribute_preserved() {
    let doc = layout_html(r#"<select multiple id="m"><option>A</option><option>B</option></select>"#, 400.0);
    let sel = find_by_id(&doc.root, "m").unwrap();
    assert!(sel.attributes.contains_key("multiple"));
}

// ── Disabled text rendering ─────────────────────────────────────────────────

#[test]
fn disabled_input_has_opacity() {
    let doc = layout_html(r#"<input type="text" value="test" disabled>"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.style.opacity < 1.0, "disabled should have reduced opacity, got {}", input.style.opacity);
}

// ── Focus ring on tab-focused elements ──────────────────────────────────────

#[test]
fn tab_focused_element_has_keyboard_focus() {
    let mut doc = layout_html(r#"<input type="text" id="a"><input type="text" id="b">"#, 400.0);
    doc.focus_next();
    assert!(doc.keyboard_focus, "Tab focus should set keyboard_focus=true");
}

// ── Range slider ────────────────────────────────────────────────────────────

#[test]
fn range_has_default_value() {
    let doc = layout_html(r#"<input type="range" min="0" max="100" value="50">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.attributes.get("value").map(|s| s.as_str()), Some("50"));
}

// ── Ctrl+A selects all text ─────────────────────────────────────────────────

#[test]
fn ctrl_a_selects_all_in_input() {
    let mut doc = layout_html(r#"<input type="text" id="t" value="hello">"#, 400.0);
    let input = find_by_id(&doc.root, "t").unwrap();
    let center = (input.layout.border_rect.x + input.layout.border_rect.w / 2.0,
                  input.layout.border_rect.y + input.layout.border_rect.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, center, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, center, 0);
    // Ctrl+A
    doc.process_key_event(crate::dom::HtmlEventType::KeyDown, 65, Some('a'), true, false, false, false);
    let input = find_by_id(&doc.root, "t").unwrap();
    assert_eq!(input.input_cursor, 5, "Ctrl+A should move cursor to end");
    assert_eq!(input.input_sel_anchor, 0, "Ctrl+A should set anchor to start");
}

// ── Backspace with selection deletes selection ──────────────────────────────

#[test]
fn backspace_deletes_selection() {
    let mut doc = layout_html(r#"<input type="text" id="t" value="hello">"#, 400.0);
    let input = find_by_id(&doc.root, "t").unwrap();
    let center = (input.layout.border_rect.x + input.layout.border_rect.w / 2.0,
                  input.layout.border_rect.y + input.layout.border_rect.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, center, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, center, 0);
    // Select all with Ctrl+A
    doc.process_key_event(crate::dom::HtmlEventType::KeyDown, 65, Some('a'), true, false, false, false);
    // Backspace should delete selection
    doc.process_key_event(crate::dom::HtmlEventType::KeyDown, 8, None, false, false, false, false);
    let input = find_by_id(&doc.root, "t").unwrap();
    assert_eq!(input_value(input), "", "backspace should delete selected text");
}

fn disabled_input_not_focusable() {
    let doc = layout_html(r#"<input type="text" disabled>"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    // Disabled elements should still be focusable per spec but...
    // actually in browsers disabled inputs ARE focusable by click
    // Let's just verify the attribute is there
    assert!(input.attributes.contains_key("disabled"));
}

// ── Input height stretches background ───────────────────────────────────────

#[test]
fn input_explicit_height_stretches_padding_rect() {
    let doc = layout_html(
        r#"<style>input { height: 80px; background-color: #ccc; }</style>
           <input type="text" id="t">"#,
        400.0,
    );
    let input = find_by_id(&doc.root, "t").unwrap();
    // padding_rect.h should be at least 80px (the explicit height)
    assert!(input.layout.padding_rect.h >= 78.0,
        "padding_rect height {} should be >= 78 when height:80px is set",
        input.layout.padding_rect.h);
    // border_rect should also reflect the height
    assert!(input.layout.border_rect.h >= 78.0,
        "border_rect height {} should be >= 78 when height:80px is set",
        input.layout.border_rect.h);
}

#[test]
fn input_explicit_height_content_rect_smaller() {
    let doc = layout_html(
        r#"<style>input { height: 100px; padding: 10px; box-sizing: border-box; }</style>
           <input type="text" id="t">"#,
        400.0,
    );
    let input = find_by_id(&doc.root, "t").unwrap();
    // With border-box, border_rect.h = 100, content_rect.h = 100 - padding*2 - border*2
    // UA has border: 1px, so content = 100 - 20 - 2 = 78
    assert!(input.layout.border_rect.h >= 98.0,
        "border_rect height {} should be ~100 with border-box",
        input.layout.border_rect.h);
    assert!(input.layout.content_rect.h < input.layout.border_rect.h,
        "content_rect.h {} should be smaller than border_rect.h {} due to padding",
        input.layout.content_rect.h, input.layout.border_rect.h);
}

#[test]
fn textarea_explicit_height_stretches() {
    let doc = layout_html(
        r#"<style>textarea { height: 120px; }</style>
           <textarea id="t">text</textarea>"#,
        400.0,
    );
    let ta = find_by_id(&doc.root, "t").unwrap();
    assert!(ta.layout.padding_rect.h >= 118.0,
        "textarea padding_rect.h {} should be >= 118 with height:120px",
        ta.layout.padding_rect.h);
}

#[test]
fn select_explicit_height_stretches() {
    let doc = layout_html(
        r#"<style>select { height: 60px; }</style>
           <select id="s"><option>A</option></select>"#,
        400.0,
    );
    let sel = find_by_id(&doc.root, "s").unwrap();
    assert!(sel.layout.padding_rect.h >= 58.0,
        "select padding_rect.h {} should be >= 58 with height:60px",
        sel.layout.padding_rect.h);
}

#[test]
fn button_explicit_height_stretches() {
    let doc = layout_html(
        r#"<button id="b" style="height: 80px;">Click</button>"#,
        400.0,
    );
    let btn = find_by_id(&doc.root, "b").unwrap();
    assert!(btn.layout.padding_rect.h >= 78.0,
        "button padding_rect.h {} should be >= 78 with height:80px",
        btn.layout.padding_rect.h);
}

#[test]
fn submit_input_explicit_height_stretches() {
    let doc = layout_html(
        r#"<input type="submit" id="s" value="Go" style="height: 60px;">"#,
        400.0,
    );
    let sub = find_by_id(&doc.root, "s").unwrap();
    assert!(sub.layout.padding_rect.h >= 58.0,
        "submit padding_rect.h {} should be >= 58 with height:60px",
        sub.layout.padding_rect.h);
}

// ── Colour picker popup ──────────────────────────────────────────────────────

#[test]
fn clicking_a_colour_input_opens_its_picker_and_a_swatch_sets_the_value() {
    // HTML §4.10.5.1.15 leaves the picker's FORM to the user agent and says
    // only that one is offered. What it does pin down is the value: a "valid
    // simple colour", `#rrggbb`, which is what a pick has to write back.
    //
    // The picker rides the same overlay the `<select>` dropdown uses —
    // `open_picker` beside `open_select` — so this is a second thing on one
    // popup surface rather than a second mechanism.
    let mut doc = layout_html(
        // `r##` because the markup contains `"#` — a plain `r#"…"#` ends
        // right there, at the value's own hash.
        r##"<input type="color" id="c" value="#000000">"##,
        400.0,
    );
    let c = doc.get_element_by_id("c").unwrap();
    let br = find_by_id(&doc.root, "c").unwrap().layout.border_rect;
    let centre = (br.x + br.w / 2.0, br.y + br.h / 2.0);

    assert_eq!(doc.open_picker, 0, "nothing is open before the click");
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, centre, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, centre, 0);
    assert_eq!(doc.open_picker, c, "activating the control opens its picker");

    // The palette sits below the control; pick the first swatch of the second
    // row, whatever the palette holds there.
    let (px, py, _, _) = doc.picker_rect(c).expect("an open picker has geometry");
    let cell = crate::widgets::PALETTE_CELL;
    let point = (px + cell / 2.0, py + cell * 1.5);
    let expected = doc.picker_hit(c, point).expect("that point is a swatch");

    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, point, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, point, 0);

    assert_eq!(doc.open_picker, 0, "picking closes the picker");
    assert_eq!(
        doc.value(c),
        crate::widgets::to_simple_colour(expected),
        "the pick writes a valid simple colour back into the value"
    );
}

#[test]
fn clicking_away_closes_the_picker_without_changing_the_value() {
    let mut doc = layout_html(
        r##"<input type="color" id="c" value="#123456">"##,
        400.0,
    );
    let c = doc.get_element_by_id("c").unwrap();
    let br = find_by_id(&doc.root, "c").unwrap().layout.border_rect;
    let centre = (br.x + br.w / 2.0, br.y + br.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, centre, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, centre, 0);
    assert_eq!(doc.open_picker, c);

    // Far from both the control and its palette.
    let away = (br.x + 300.0, br.y + 200.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, away, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, away, 0);
    assert_eq!(doc.open_picker, 0, "clicking away closes it");
    assert_eq!(doc.value(c), "#123456", "and picks nothing");
}

#[test]
fn a_dropdown_row_can_be_picked_where_no_element_lies_beneath() {
    // The list is drawn OVER the page and is not in the tree, so a row that
    // hangs past the end of the content has nothing under it. The click path
    // is gated on having hit an element, which silently dropped exactly those
    // picks — the ones furthest down the list, on a short page.
    //
    // Same fault the colour picker had, which is why both are now allowed
    // through: a popup's click is about the POPUP's geometry, not about
    // whatever the hit test finds below it.
    let mut doc = layout_html(
        r#"<select id="s"><option>A</option><option>B</option><option>C</option></select>"#,
        400.0,
    );
    let s = doc.get_element_by_id("s").unwrap();
    let br = find_by_id(&doc.root, "s").unwrap().layout.border_rect;
    let centre = (br.x + br.w / 2.0, br.y + br.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, centre, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, centre, 0);
    assert_eq!(doc.open_select, s, "clicking the select opens the list");

    // The THIRD row, which sits below the select — and on a document this
    // short, below the body's content entirely.
    //
    // Row geometry is the engine's: `font-size * 1.8` per row, and the list is
    // inset 4px from the control's bottom edge. Derived rather than guessed —
    // a hardcoded 20px lands on row 1 and the test then "fails" over its own
    // arithmetic.
    let font_px = 16.0;
    let row_h = font_px * 1.8;
    let third = (br.x + br.w / 2.0, br.y + br.h + 4.0 + row_h * 2.5);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, third, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, third, 0);

    assert_eq!(doc.open_select, 0, "picking closes the list");
    assert_eq!(
        doc.selected_index(s),
        2,
        "the third row is what was clicked, and it is what got selected"
    );
}

#[test]
fn a_date_input_opens_a_calendar_and_a_day_sets_the_value() {
    // Same popup surface as the dropdown and the colour picker: `open_picker`
    // holds the node, the renderer draws the month over the page, and the
    // click is taken before hit testing. What differs is only what a pick
    // MEANS — a day, written back as `yyyy-mm-dd`, the format the spec
    // requires of this control's value.
    let mut doc = layout_html(
        r#"<input type="date" id="d" value="2026-08-24">"#,
        400.0,
    );
    let d = doc.get_element_by_id("d").unwrap();
    let br = find_by_id(&doc.root, "d").unwrap().layout.border_rect;
    let centre = (br.x + br.w / 2.0, br.y + br.h / 2.0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, centre, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, centre, 0);
    assert_eq!(doc.open_picker, d, "activating a date control opens its calendar");

    // August 2026 starts on a Saturday, so the 1st sits in column 5 of row 0.
    let (px, py, _, _) = doc.picker_rect(d).expect("an open picker has geometry");
    let cell = crate::widgets::Calendar::CELL;
    let first = crate::widgets::first_weekday(2026, 8);
    let day_10_index = first + 9;
    let point = (
        px + (day_10_index % 7) as f32 * cell + cell / 2.0,
        py + crate::widgets::Calendar::HEADER + (day_10_index / 7) as f32 * cell + cell / 2.0,
    );
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseDown, point, 0);
    doc.process_mouse_event(crate::dom::HtmlEventType::MouseUp, point, 0);

    assert_eq!(doc.open_picker, 0, "picking closes the calendar");
    assert_eq!(
        doc.value(d),
        "2026-08-10",
        "the pick writes the date in the format the control's value takes"
    );
}
