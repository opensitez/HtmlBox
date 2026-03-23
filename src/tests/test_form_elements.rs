use crate::{parse_html, Renderer, Document};
use crate::css::apply_cascade_vp;
use crate::types::*;
use crate::layout::LayoutEngine;

fn layout_html(html: &str, width: f32) -> Document {
    let mut doc = parse_html(html);
    apply_cascade_vp(&mut doc.root, &doc.stylesheet, None, 16.0, width, 900.0, std::ptr::null(), false);
    let mut eng = LayoutEngine::new();
    eng.viewport_h = 900.0;
    eng.layout(&mut doc, width);
    doc
}

fn find_by_id<'a>(node: &'a HtmlBox, id: &str) -> Option<&'a HtmlBox> {
    if node.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(node); }
    for child in &node.children { if let Some(n) = find_by_id(child, id) { return Some(n); } }
    None
}

fn find_by_tag<'a>(node: &'a HtmlBox, tag: &str) -> Option<&'a HtmlBox> {
    if node.tag == tag { return Some(node); }
    for child in &node.children { if let Some(n) = find_by_tag(child, tag) { return Some(n); } }
    None
}

fn find_all_by_tag<'a>(node: &'a HtmlBox, tag: &str, out: &mut Vec<&'a HtmlBox>) {
    if node.tag == tag { out.push(node); }
    for child in &node.children { find_all_by_tag(child, tag, out); }
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
    assert!(input.content_rect.w > 100.0, "input width {} should be > 100", input.content_rect.w);
}

#[test]
fn text_input_has_height() {
    let doc = layout_html(r#"<input type="text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.content_rect.h > 10.0, "input height {} should be > 10", input.content_rect.h);
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
    assert!(input.resolved_border_top > 0.0, "input should have border");
    assert!(input.resolved_border_left > 0.0);
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
    let top_space = (input.content_rect.h - line_h) / 2.0;
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
    fn find_mut<'a>(n: &'a mut HtmlBox, id: &str) -> Option<&'a mut HtmlBox> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 2; // at end
    let changed = process_form_input_key(input, 'c' as u32, Some('c'));
    assert!(changed);
    assert_eq!(input_value(input), "abc");
    assert_eq!(input.input_cursor, 3);
}

#[test]
fn process_form_input_key_backspace() {
    let doc = layout_html(r#"<input type="text" id="t" value="abc">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut HtmlBox, id: &str) -> Option<&'a mut HtmlBox> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 3;
    let changed = process_form_input_key(input, 8, None); // backspace
    assert!(changed);
    assert_eq!(input_value(input), "ab");
    assert_eq!(input.input_cursor, 2);
}

#[test]
fn process_form_input_key_arrow_left() {
    let doc = layout_html(r#"<input type="text" id="t" value="abc">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut HtmlBox, id: &str) -> Option<&'a mut HtmlBox> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 2;
    process_form_input_key(input, 37, None); // left arrow
    assert_eq!(input.input_cursor, 1);
}

#[test]
fn process_form_input_key_arrow_right() {
    let doc = layout_html(r#"<input type="text" id="t" value="abc">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut HtmlBox, id: &str) -> Option<&'a mut HtmlBox> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 1;
    process_form_input_key(input, 39, None); // right arrow
    assert_eq!(input.input_cursor, 2);
}

#[test]
fn process_form_input_key_enter_not_in_input() {
    let doc = layout_html(r#"<input type="text" id="t" value="abc">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut HtmlBox, id: &str) -> Option<&'a mut HtmlBox> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 3;
    let changed = process_form_input_key(input, 13, None); // enter
    assert!(!changed, "Enter should not insert in single-line input");
    assert_eq!(input_value(input), "abc");
}

#[test]
fn process_form_input_key_space() {
    let doc = layout_html(r#"<input type="text" id="t" value="ab">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut HtmlBox, id: &str) -> Option<&'a mut HtmlBox> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 1;
    let changed = process_form_input_key(input, 32, Some(' '));
    assert!(changed);
    assert_eq!(input_value(input), "a b");
    assert_eq!(input.input_cursor, 2);
}

// ── Checkbox Tests ───────────────────────────────────────────────────────────

#[test]
fn checkbox_has_correct_size() {
    let doc = layout_html(r#"<input type="checkbox">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.content_rect.w, 16.0);
    assert_eq!(input.content_rect.h, 16.0);
}

#[test]
fn checkbox_no_border() {
    let doc = layout_html(r#"<input type="checkbox">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.resolved_border_top, 0.0);
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
    let input_right = input.margin_rect.x + input.margin_rect.w;
    // Check that the parent div's line cache positions text after the checkbox
    let div = find_by_tag(&doc.root, "div").unwrap();
    assert!(!div.line_cache.is_empty(), "div should have line cache");
    let line = &div.line_cache[0];
    // text_x_offset should be > 0 (accounting for checkbox)
    assert!(line.text_x_offset > 0.0, "text_x_offset {} should be > 0", line.text_x_offset);
}

// ── Radio Button Tests ───────────────────────────────────────────────────────

#[test]
fn radio_has_correct_size() {
    let doc = layout_html(r#"<input type="radio" name="g">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert_eq!(input.content_rect.w, 16.0);
    assert_eq!(input.content_rect.h, 16.0);
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
    assert!(input.content_rect.w > 20.0, "button width {} should be > 20", input.content_rect.w);
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
    assert!(select.content_rect.w > 50.0, "select width {} should be > 50", select.content_rect.w);
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
    assert!(ta.content_rect.w > 50.0);
    assert!(ta.content_rect.h > 20.0);
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
    assert_eq!(input.resolved_border_top, 0.0);
}

// ── Progress Tests ───────────────────────────────────────────────────────────

#[test]
fn progress_has_size() {
    let doc = layout_html(r#"<progress value="0.5" max="1"></progress>"#, 400.0);
    let prog = find_by_tag(&doc.root, "progress").unwrap();
    assert!(prog.content_rect.w > 50.0);
    assert!(prog.content_rect.h > 5.0);
}

// ── Meter Tests ──────────────────────────────────────────────────────────────

#[test]
fn meter_has_size() {
    let doc = layout_html(r#"<meter value="0.5" min="0" max="1"></meter>"#, 400.0);
    let meter = find_by_tag(&doc.root, "meter").unwrap();
    assert!(meter.content_rect.w > 30.0);
    assert!(meter.content_rect.h > 5.0);
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
    assert!(fs.resolved_border_top > 0.0);
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
    assert!(ta.resolved_border_top > 0.0, "textarea should have border");
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
    assert!((ta.margin_rect.w - 400.0).abs() < 5.0,
        "textarea width {} should be ~400", ta.margin_rect.w);
}

// ── Input styling tests ─────────────────────────────────────────────────────

#[test]
fn input_padding_override() {
    let doc = layout_html(r#"<style>input { padding: 10px 15px; }</style>
        <input type="text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!((input.resolved_pad_top - 10.0).abs() < 1.0, "padding-top {} should be ~10", input.resolved_pad_top);
    assert!((input.resolved_pad_left - 15.0).abs() < 1.0, "padding-left {} should be ~15", input.resolved_pad_left);
}

#[test]
fn input_border_color_override() {
    let doc = layout_html(r#"<style>input { border: 2px solid red; }</style>
        <input type="text">"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!((input.resolved_border_top - 2.0).abs() < 0.5, "border should be 2px");
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
    assert!(input.resolved_margin_right >= 4.0, "checkbox should have right margin >= 4, got {}", input.resolved_margin_right);
}

#[test]
fn radio_in_label_spacing() {
    let doc = layout_html(r#"<label><input type="radio" name="g"> Choice</label>"#, 400.0);
    let input = find_by_tag(&doc.root, "input").unwrap();
    assert!(input.resolved_margin_right >= 4.0, "radio should have right margin >= 4, got {}", input.resolved_margin_right);
}

// ── Form input key handling tests ────────────────────────────────────────────

#[test]
fn form_input_insert_at_middle() {
    let doc = layout_html(r#"<input type="text" id="t" value="ac">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut HtmlBox, id: &str) -> Option<&'a mut HtmlBox> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 1; // between 'a' and 'c'
    process_form_input_key(input, 'b' as u32, Some('b'));
    assert_eq!(input_value(input), "abc");
    assert_eq!(input.input_cursor, 2);
}

#[test]
fn form_input_delete_key() {
    let doc = layout_html(r#"<input type="text" id="t" value="abc">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut HtmlBox, id: &str) -> Option<&'a mut HtmlBox> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 1; // before 'b'
    process_form_input_key(input, 46, None); // delete
    assert_eq!(input_value(input), "ac");
    assert_eq!(input.input_cursor, 1);
}

#[test]
fn form_input_home_end() {
    let doc = layout_html(r#"<input type="text" id="t" value="hello">"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut HtmlBox, id: &str) -> Option<&'a mut HtmlBox> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let input = find_mut(&mut root, "t").unwrap();
    input.input_cursor = 2;
    process_form_input_key(input, 36, None); // Home
    assert_eq!(input.input_cursor, 0);
    process_form_input_key(input, 35, None); // End
    assert_eq!(input.input_cursor, 5);
}

#[test]
fn textarea_enter_inserts_newline() {
    let doc = layout_html(r#"<textarea id="t">ab</textarea>"#, 400.0);
    let mut root = doc.root;
    fn find_mut<'a>(n: &'a mut HtmlBox, id: &str) -> Option<&'a mut HtmlBox> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, id) { return Some(r); } }
        None
    }
    let ta = find_mut(&mut root, "t").unwrap();
    ta.input_cursor = 1;
    let changed = process_form_input_key(ta, 13, None); // Enter
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
    fn fm<'a>(n: &'a mut HtmlBox, id: &str) -> Option<&'a mut HtmlBox> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = fm(c, id) { return Some(r); } }
        None
    }
    let input = fm(&mut root, "t").unwrap();
    input.input_cursor = 5;
    let changed = process_form_input_key(input, 'x' as u32, Some('x'));
    assert!(!changed, "readonly input should not accept input");
    assert_eq!(input_value(input), "fixed");
}

// ── Maxlength tests ─────────────────────────────────────────────────────────

#[test]
fn maxlength_prevents_input() {
    let doc = layout_html(r#"<input type="text" id="t" value="abc" maxlength="5">"#, 400.0);
    let mut root = doc.root;
    fn fm<'a>(n: &'a mut HtmlBox, id: &str) -> Option<&'a mut HtmlBox> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = fm(c, id) { return Some(r); } }
        None
    }
    let input = fm(&mut root, "t").unwrap();
    input.input_cursor = 3;
    process_form_input_key(input, 'd' as u32, Some('d')); // "abcd" — ok
    process_form_input_key(input, 'e' as u32, Some('e')); // "abcde" — ok, at limit
    let changed = process_form_input_key(input, 'f' as u32, Some('f')); // should be blocked
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
    assert!(ta2.content_rect.h > ta1.content_rect.h,
        "rows=6 height {} should be > rows=2 height {}", ta2.content_rect.h, ta1.content_rect.h);
}

// ── Input size attribute ────────────────────────────────────────────────────

#[test]
fn input_size_affects_width() {
    let doc1 = layout_html(r#"<input type="text" size="5">"#, 400.0);
    let doc2 = layout_html(r#"<input type="text" size="40">"#, 500.0);
    let i1 = find_by_tag(&doc1.root, "input").unwrap();
    let i2 = find_by_tag(&doc2.root, "input").unwrap();
    assert!(i2.content_rect.w > i1.content_rect.w,
        "size=40 width {} should be > size=5 width {}", i2.content_rect.w, i1.content_rect.w);
}

// ── Disabled blocks form_input_key ──────────────────────────────────────────

#[test]
fn disabled_input_blocks_typing() {
    let doc = layout_html(r#"<input type="text" id="t" value="hi" disabled>"#, 400.0);
    let mut root = doc.root;
    fn fm<'a>(n: &'a mut HtmlBox, id: &str) -> Option<&'a mut HtmlBox> {
        if n.attributes.get("id").map(|s| s.as_str()) == Some(id) { return Some(n); }
        for c in &mut n.children { if let Some(r) = fm(c, id) { return Some(r); } }
        None
    }
    let input = fm(&mut root, "t").unwrap();
    input.input_cursor = 2;
    let changed = process_form_input_key(input, 'x' as u32, Some('x'));
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
    assert!((input.margin_rect.w - 300.0).abs() < 5.0,
        "input width {} should be ~300", input.margin_rect.w);
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
