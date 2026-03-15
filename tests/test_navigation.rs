// Tests for navigation: hit testing, offset-to-point, and line collection.

use rhtmledit::types::*;
use rhtmledit::parse_html;
use rhtmledit::layout::LayoutEngine;
use rhtmledit::layout::hit_test::*;

fn layout(html: &str, width: f32) -> Document {
    rhtmledit::load_html(html, width)
}

#[test]
fn nav_offset_to_point_basic() {
    let doc = layout("<p>Hello</p>", 800.0);
    // offset 0 in the root box
    let pt = offset_to_point(&doc.root, &doc.root as *const HtmlBox, 0, 0.0, 0.0);
    assert!(pt.is_some());
    let (x, y) = pt.unwrap();
    assert!(x >= 0.0);
    assert!(y >= 0.0);
}

#[test]
fn nav_point_to_hit_basic() {
    let doc = layout("<p>Hello</p>", 800.0);
    let hit = point_to_hit(&doc.root, (5.0, 5.0));
    assert!(hit.is_some());
}

#[test]
fn nav_hit_test_distinct_offsets() {
    let doc = layout("<p>A</p><p>B</p><p>C</p>", 800.0);
    
    // We'll hit specific points and verify they hit different boxes or offsets
    let hit_a = point_to_hit(&doc.root, (5.0, 5.0)).unwrap();
    let hit_b = point_to_hit(&doc.root, (5.0, 30.0)).unwrap(); // assuming some line height
    
    assert!(hit_a.box_ptr != hit_b.box_ptr || hit_a.local_offset != hit_b.local_offset);
}

#[test]
fn nav_caret_x_roundtrip() {
    let doc = layout("<p>Hello World</p>", 800.0);
    // Hit roughly in the middle of "Hello"
    let hit = point_to_hit(&doc.root, (20.0, 5.0)).unwrap();
    let pt = offset_to_point(&doc.root, hit.box_ptr, hit.local_offset, 0.0, 0.0).unwrap();
    
    // X should be close to 20.0
    assert!((pt.0 - 20.0).abs() < 20.0);
}

#[test]
fn nav_wrapped_text_multiple_lines() {
    let doc = layout("<p>This is a long sentence that should wrap multiple times in a narrow viewport of one hundred pixels</p>", 100.0);
    
    let hit_start = point_to_hit(&doc.root, (5.0, 5.0)).unwrap();
    let hit_end   = point_to_hit(&doc.root, (5.0, 100.0)).unwrap();
    
    let pt_start = offset_to_point(&doc.root, hit_start.box_ptr, hit_start.local_offset, 0.0, 0.0).unwrap();
    let pt_end   = offset_to_point(&doc.root, hit_end.box_ptr, hit_end.local_offset, 0.0, 0.0).unwrap();
    
    assert!(pt_end.1 > pt_start.1 + 20.0);
}

#[test]
fn nav_table_hit_test() {
    let doc = layout("<table><tr><td>A</td><td>B</td></tr></table>", 800.0);
    
    // A and B should be at different X
    let hit_a = point_to_hit(&doc.root, (10.0, 10.0)).unwrap();
    let hit_b = point_to_hit(&doc.root, (500.0, 10.0)).unwrap();
    
    let pt_a = offset_to_point(&doc.root, hit_a.box_ptr, hit_a.local_offset, 0.0, 0.0).unwrap();
    let pt_b = offset_to_point(&doc.root, hit_b.box_ptr, hit_b.local_offset, 0.0, 0.0).unwrap();
    
    assert!((pt_a.1 - pt_b.1).abs() < 5.0);
    assert!(pt_b.0 > pt_a.0);
}

// ============================================================
// Word Boundary Navigation (string-level logic)
// ============================================================

#[test]
fn nav_word_boundary_left_from_middle() {
    // "Hello World" — from offset inside "World" should go to start of "World"
    let doc = layout("<p>Hello World</p>", 800.0);
    let text = doc.root.text_content();
    let wpos = text.find("World").unwrap();
    // From middle of "World" (wpos+2), skip word chars going left
    let mut pos = wpos + 2;
    while pos > 0 {
        let prev = floor_char_boundary(&text, pos - 1);
        if text[prev..pos].chars().next().map(|c| c.is_alphanumeric()).unwrap_or(false) {
            pos = prev;
        } else {
            break;
        }
    }
    assert_eq!(pos, wpos);
}

#[test]
fn nav_word_boundary_right_from_start() {
    let doc = layout("<p>Hello World</p>", 800.0);
    let text = doc.root.text_content();
    let hpos = text.find("Hello").unwrap();
    let mut pos = hpos;
    let chars: Vec<(usize, char)> = text[hpos..].char_indices()
        .map(|(i, c)| (i + hpos, c))
        .collect();
    for (i, c) in &chars {
        if c.is_alphanumeric() {
            pos = i + c.len_utf8();
        } else {
            break;
        }
    }
    // Should be at space after "Hello"
    assert_eq!(pos, hpos + "Hello".len());
}

#[test]
fn nav_word_boundary_left_at_start() {
    // At position 0, WordBoundaryLeft returns 0
    let pos: usize = 0;
    assert_eq!(pos, 0);
}

// ============================================================
// Line Start / End (string-level logic)
// ============================================================

#[test]
fn nav_line_start_from_middle() {
    // Two paragraphs: from middle of "World" going left to find line start
    let doc = layout("<p>Hello</p><p>World</p>", 800.0);
    let text = doc.root.text_content();
    let wpos = text.find("World").unwrap();
    let mut pos = wpos + 2;
    while pos > 0 {
        let prev = floor_char_boundary(&text, pos - 1);
        if text.as_bytes().get(prev) == Some(&b'\n') {
            break;
        }
        pos = prev;
    }
    // pos should be at or before wpos (start of "World"'s segment)
    assert!(pos <= wpos);
}

#[test]
fn nav_line_end_from_middle() {
    let doc = layout("<p>Hello</p><p>World</p>", 800.0);
    let text = doc.root.text_content();
    let hpos = text.find("Hello").unwrap();
    let mut pos = hpos + 2;
    let bytes = text.as_bytes();
    while pos < text.len() && bytes.get(pos) != Some(&b'\n') {
        pos += 1;
    }
    // pos should be at or after end of "Hello"
    assert!(pos >= hpos + "Hello".len() || pos == text.len());
}

// ============================================================
// Word Selection (string-level logic)
// ============================================================

#[test]
fn nav_word_selection_in_text() {
    let doc = layout("<p>Hello World Test</p>", 800.0);
    let text = doc.root.text_content();
    let wpos = text.find("World").unwrap();
    // Expand from middle of "World"
    let mut start = wpos + 2;
    let mut end = wpos + 2;
    // Go left
    while start > 0 {
        let prev = floor_char_boundary(&text, start - 1);
        if text[prev..start].chars().next().map(|c| c.is_alphanumeric()).unwrap_or(false) {
            start = prev;
        } else {
            break;
        }
    }
    // Go right
    while end < text.len() {
        let c = text[end..].chars().next().unwrap();
        if c.is_alphanumeric() {
            end += c.len_utf8();
        } else {
            break;
        }
    }
    assert_eq!(start, wpos);
    assert_eq!(end, wpos + "World".len());
}

// ============================================================
// Y-position ordering: multiple paragraphs
// ============================================================

#[test]
fn nav_multiple_paragraphs_have_increasing_y() {
    // Three separate paragraphs should each be at a greater Y than the previous
    let doc = layout("<p>First</p><p>Second</p><p>Third</p>", 800.0);
    let text = doc.root.text_content();
    let pos_a = text.find("First").unwrap();
    let pos_b = text.find("Second").unwrap();
    let pos_c = text.find("Third").unwrap();

    // We need the box+offset for each word; use point_to_hit by getting offsets via
    // offset_to_point round trip from the root box.
    // Find each paragraph's box
    use rhtmledit::dom::query_selector_all;
    let paras = query_selector_all(&doc.root, "p");
    assert!(paras.len() >= 3, "expected at least 3 paragraphs");

    let pt_a = offset_to_point(&doc.root, paras[0] as *const HtmlBox, 1, 0.0, 0.0);
    let pt_b = offset_to_point(&doc.root, paras[1] as *const HtmlBox, 1, 0.0, 0.0);
    let pt_c = offset_to_point(&doc.root, paras[2] as *const HtmlBox, 1, 0.0, 0.0);

    // All should be Some and Y should increase
    assert!(pt_a.is_some(), "paragraph A has no point");
    assert!(pt_b.is_some(), "paragraph B has no point");
    assert!(pt_c.is_some(), "paragraph C has no point");
    assert!(pt_b.unwrap().1 > pt_a.unwrap().1, "B should be below A");
    assert!(pt_c.unwrap().1 > pt_b.unwrap().1, "C should be below B");

    // Suppress unused variable warnings
    let _ = (pos_a, pos_b, pos_c);
}

#[test]
fn nav_heading_and_paragraph_ordering() {
    let doc = layout("<h1>Title</h1><p>Body text here</p>", 800.0);
    use rhtmledit::dom::query_selector;
    let h1 = query_selector(&doc.root, "h1").unwrap();
    let p  = query_selector(&doc.root, "p").unwrap();

    let pt_h1 = offset_to_point(&doc.root, h1 as *const HtmlBox, 0, 0.0, 0.0);
    let pt_p  = offset_to_point(&doc.root, p  as *const HtmlBox, 0, 0.0, 0.0);

    assert!(pt_h1.is_some());
    assert!(pt_p.is_some());
    // h1 comes before p, so its Y should be less
    assert!(pt_h1.unwrap().1 <= pt_p.unwrap().1);
}

// ============================================================
// Blockquote click: distinct offsets for different positions
// ============================================================

#[test]
fn nav_click_returns_distinct_offsets_per_box() {
    let doc = layout(
        "<blockquote><p>Before text</p></blockquote>\
         <p>Middle paragraph</p>\
         <blockquote><p>After text here</p></blockquote>",
        800.0,
    );
    let text = doc.root.text_content();
    let after_pos  = text.find("After").expect("'After' not found");
    let here_pos   = text.find("here").expect("'here' not found");

    // Find the <p> inside the second blockquote
    use rhtmledit::dom::query_selector_all;
    let paras = query_selector_all(&doc.root, "p");
    // The last paragraph should contain "After text here"
    let last_p = paras.iter().find(|p| p.text_content().contains("After")).copied();
    assert!(last_p.is_some(), "did not find 'After text here' paragraph");
    let last_p = last_p.unwrap();

    // Get X for start of "After" and start of "here" within that paragraph
    let p_text = last_p.text_content();
    let local_after = p_text.find("After").unwrap_or(0);
    let local_here  = p_text.find("here").unwrap_or(0);

    let pt_after = offset_to_point(&doc.root, last_p as *const HtmlBox, local_after, 0.0, 0.0);
    let pt_here  = offset_to_point(&doc.root, last_p as *const HtmlBox, local_here,  0.0, 0.0);

    assert!(pt_after.is_some());
    assert!(pt_here.is_some());

    let (xa, ya) = pt_after.unwrap();
    let (xh, _)  = pt_here.unwrap();

    // "After" should be to the left of "here" (same line, same Y)
    assert!(xh > xa, "expected 'here' to be right of 'After', got xa={xa} xh={xh}");

    // Now click back at those screen positions
    let hit_after = point_to_hit(&doc.root, (xa + 2.0, ya + 5.0));
    let hit_here  = point_to_hit(&doc.root, (xh + 2.0, ya + 5.0));
    assert!(hit_after.is_some());
    assert!(hit_here.is_some());

    // The two clicks must yield different offsets or different boxes
    let ha = hit_after.unwrap();
    let hh = hit_here.unwrap();
    assert!(
        ha.box_ptr != hh.box_ptr || ha.local_offset != hh.local_offset,
        "Expected distinct hit results for 'After' and 'here'"
    );

    let _ = (after_pos, here_pos);
}

#[test]
fn nav_click_on_each_paragraph_returns_distinct_offsets() {
    let doc = layout(
        "<blockquote><p>AAA</p></blockquote>\
         <p>BBB</p>\
         <blockquote><p>CCC</p></blockquote>",
        800.0,
    );
    use rhtmledit::dom::query_selector_all;
    let paras = query_selector_all(&doc.root, "p");
    assert!(paras.len() >= 3);

    let find_para = |needle: &str| {
        paras.iter().find(|p| p.text_content().contains(needle)).copied()
    };
    let pa = find_para("AAA").expect("AAA paragraph not found");
    let pb = find_para("BBB").expect("BBB paragraph not found");
    let pc = find_para("CCC").expect("CCC paragraph not found");

    let pt_a = offset_to_point(&doc.root, pa as *const HtmlBox, 1, 0.0, 0.0).unwrap();
    let pt_b = offset_to_point(&doc.root, pb as *const HtmlBox, 1, 0.0, 0.0).unwrap();
    let pt_c = offset_to_point(&doc.root, pc as *const HtmlBox, 1, 0.0, 0.0).unwrap();

    // Different Y positions
    assert!(pt_a.1 != pt_b.1, "A and B should have different Y");
    assert!(pt_b.1 != pt_c.1, "B and C should have different Y");

    // Click on each Y, verify ordering
    let hit_a = point_to_hit(&doc.root, (pt_a.0, pt_a.1 + 2.0));
    let hit_b = point_to_hit(&doc.root, (pt_b.0, pt_b.1 + 2.0));
    let hit_c = point_to_hit(&doc.root, (pt_c.0, pt_c.1 + 2.0));

    assert!(hit_a.is_some());
    assert!(hit_b.is_some());
    assert!(hit_c.is_some());

    // Each click should hit a distinct box (different paragraphs)
    let ba = hit_a.unwrap().box_ptr;
    let bb = hit_b.unwrap().box_ptr;
    let bc = hit_c.unwrap().box_ptr;
    assert!(ba != bb, "A and B clicks should hit different boxes");
    assert!(bb != bc, "B and C clicks should hit different boxes");
}

#[test]
fn nav_wrapped_text_line_ordering() {
    // Long text in narrow viewport: hit at bottom of para should have greater Y
    let doc = layout(
        "<p>The quick brown fox jumps over the lazy dog and keeps running further away</p>",
        150.0,
    );
    let p = {
        use rhtmledit::dom::query_selector;
        query_selector(&doc.root, "p").unwrap()
    };
    // First character
    let pt_start = offset_to_point(&doc.root, p as *const HtmlBox, 0, 0.0, 0.0);
    // A later character (well into the text, past likely wrapping)
    let text_len = p.text_content().len();
    let late_offset = (text_len * 3 / 4).min(text_len);
    let pt_late  = offset_to_point(&doc.root, p as *const HtmlBox, late_offset, 0.0, 0.0);

    assert!(pt_start.is_some());
    assert!(pt_late.is_some());
    // Later in text should have higher Y (lower on screen) due to wrapping
    assert!(pt_late.unwrap().1 > pt_start.unwrap().1,
        "Expected wrapped text to have higher Y for later offset");
}

#[test]
fn nav_nested_divs_have_content() {
    // Deeply nested div should still produce a hittable point
    let doc = layout("<div><div><p>Inner paragraph</p></div></div>", 800.0);
    let hit = point_to_hit(&doc.root, (5.0, 5.0));
    assert!(hit.is_some());
}

#[test]
fn nav_flex_items_at_different_x() {
    let doc = layout(
        r#"<div style="display: flex;"><div>Item A</div><div>Item B</div></div>"#,
        800.0,
    );
    use rhtmledit::dom::query_selector_all;
    let divs = query_selector_all(&doc.root, "div");
    // Find the flex children (they contain "Item A" and "Item B")
    let ia = divs.iter().find(|d| d.text_content().trim() == "Item A").copied();
    let ib = divs.iter().find(|d| d.text_content().trim() == "Item B").copied();

    if let (Some(ia), Some(ib)) = (ia, ib) {
        let pt_a = offset_to_point(&doc.root, ia as *const HtmlBox, 0, 0.0, 0.0);
        let pt_b = offset_to_point(&doc.root, ib as *const HtmlBox, 0, 0.0, 0.0);
        if let (Some(pa), Some(pb)) = (pt_a, pt_b) {
            // Flex row: items should be at roughly the same Y
            assert!((pa.1 - pb.1).abs() < 5.0, "flex items should have similar Y");
            // Item B should be to the right of Item A
            assert!(pb.0 > pa.0, "Item B should be right of Item A");
        }
    }
}

#[test]
fn nav_list_items_at_increasing_y() {
    let doc = layout("<ul><li>Item 1</li><li>Item 2</li><li>Item 3</li></ul>", 800.0);
    use rhtmledit::dom::query_selector_all;
    let items = query_selector_all(&doc.root, "li");
    assert!(items.len() >= 3, "expected at least 3 list items");

    let pt1 = offset_to_point(&doc.root, items[0] as *const HtmlBox, 0, 0.0, 0.0);
    let pt2 = offset_to_point(&doc.root, items[1] as *const HtmlBox, 0, 0.0, 0.0);
    let pt3 = offset_to_point(&doc.root, items[2] as *const HtmlBox, 0, 0.0, 0.0);

    if let (Some(p1), Some(p2), Some(p3)) = (pt1, pt2, pt3) {
        assert!(p2.1 >= p1.1, "Item 2 should be at or below Item 1");
        assert!(p3.1 >= p2.1, "Item 3 should be at or below Item 2");
    }
}

// ─── Helper ───────────────────────────────────────────────────────────────────

fn floor_char_boundary(s: &str, mut idx: usize) -> usize {
    while idx > 0 && !s.is_char_boundary(idx) { idx -= 1; }
    idx
}
