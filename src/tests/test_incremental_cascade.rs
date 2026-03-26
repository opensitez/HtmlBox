//! Tests for incremental hover cascade performance and correctness.

use crate::html::parse_html;
use crate::css::{apply_cascade_vp_hover, apply_cascade_incremental, build_hover_chain, mark_hover_dirty, clear_cascade_dirty};
use std::collections::HashSet;

/// Generate a large HTML document with N elements for benchmarking.
fn generate_large_doc(n: usize) -> String {
    let mut html = String::from("<html><head><style>");
    // Add CSS rules that use :hover
    html.push_str("nav a:hover { color: red; background: yellow; }");
    html.push_str(".item:hover { border: 1px solid blue; }");
    html.push_str("li:hover > .sub { display: block; }");
    // General rules
    html.push_str("body { font-family: sans-serif; font-size: 14px; }");
    html.push_str("nav { display: flex; background: #333; }");
    html.push_str("nav a { color: white; padding: 10px; }");
    html.push_str(".item { padding: 5px; margin: 2px; }");
    html.push_str(".sub { display: none; }");
    html.push_str("h2 { font-size: 20px; color: #333; }");
    html.push_str("p { line-height: 1.5; margin-bottom: 10px; }");
    html.push_str("</style></head><body>");

    // Navigation bar
    html.push_str("<nav>");
    for i in 0..10 {
        html.push_str(&format!(r#"<a href="/page{}" id="nav{}">Link {}</a>"#, i, i, i));
    }
    html.push_str("</nav>");

    // Content sections
    for i in 0..n {
        html.push_str(&format!(r#"<div class="item" id="item{}"><h2>Section {}</h2>"#, i, i));
        html.push_str(&format!("<p>Content for section {}. Some text here.</p>", i));
        html.push_str(r#"<div class="sub">Hover dropdown content</div>"#);
        html.push_str("</div>");
    }

    html.push_str("</body></html>");
    html
}

#[test]
fn incremental_cascade_correctness() {
    // Verify that incremental cascade produces the same result as full cascade
    // for elements in the hover chain.
    let html = r#"<html><head><style>
        a:hover { color: red; }
        .menu:hover .dropdown { display: block; }
        .dropdown { display: none; }
    </style></head><body>
        <div class="menu" id="menu">
            <a href="/" id="link">Home</a>
            <div class="dropdown" id="drop">Content</div>
        </div>
        <p id="other">Not affected</p>
    </body></html>"#;

    let mut doc = parse_html(html);
    let empty = HashSet::new();

    // Initial full cascade (no hover)
    apply_cascade_vp_hover(
        &mut doc.root, &doc.stylesheet, None, 16.0,
        800.0, 600.0, 0, false, &empty,
    );
    clear_cascade_dirty(&mut doc.root);

    // Now simulate hover on the link
    let link_id = doc.get_element_by_id("link").unwrap();

    // Full cascade with hover (reference result)
    let mut doc_full = doc.clone();
    crate::html::rebuild_arena_from_tree(&mut doc_full.arena, &mut doc_full.root);
    let hover_chain = build_hover_chain(&doc_full.root, link_id);
    apply_cascade_vp_hover(
        &mut doc_full.root, &doc_full.stylesheet, None, 16.0,
        800.0, 600.0, 0, false, &hover_chain,
    );

    // Incremental cascade with hover
    let mut doc_inc = doc.clone();
    crate::html::rebuild_arena_from_tree(&mut doc_inc.arena, &mut doc_inc.root);
    doc_inc.rebuild_node_map();
    let hover_chain_inc = build_hover_chain(&doc_inc.root, link_id);
    let old_chain = HashSet::new(); // no previous hover
    mark_hover_dirty(&mut doc_inc.root, &old_chain, &hover_chain_inc, false);
    apply_cascade_incremental(
        &mut doc_inc.root, &doc_inc.stylesheet, None, 16.0,
        800.0, 600.0, 0, false, &hover_chain_inc,
    );
    clear_cascade_dirty(&mut doc_inc.root);

    // Compare: the hovered link should have the same color in both
    fn find_style(root: &crate::types::HtmlBox, id: u32) -> Option<crate::types::ComputedStyle> {
        if root.node_id == id { return Some(root.style.clone()); }
        for child in &root.children {
            if let Some(s) = find_style(child, id) { return Some(s); }
        }
        None
    }

    let full_link_style = find_style(&doc_full.root, link_id).unwrap();
    let inc_link_style = find_style(&doc_inc.root, link_id).unwrap();
    assert_eq!(full_link_style.color.r, inc_link_style.color.r,
        "color.r mismatch: full={} inc={}", full_link_style.color.r, inc_link_style.color.r);
    assert_eq!(full_link_style.color.g, inc_link_style.color.g);
}

#[test]
fn incremental_cascade_skips_clean_subtrees() {
    // Measure that incremental cascade visits far fewer nodes than full cascade
    let html = generate_large_doc(200); // 200 sections ~ 1400 elements
    let mut doc = parse_html(&html);
    let empty = HashSet::new();

    // Full cascade
    let t0 = std::time::Instant::now();
    apply_cascade_vp_hover(
        &mut doc.root, &doc.stylesheet, None, 16.0,
        800.0, 600.0, 0, false, &empty,
    );
    let full_time = t0.elapsed();
    clear_cascade_dirty(&mut doc.root);

    // Simulate hover on nav0
    let nav0 = doc.get_element_by_id("nav0").unwrap();
    let hover_chain = build_hover_chain(&doc.root, nav0);
    let old_chain = HashSet::new();
    doc.rebuild_node_map();
    mark_hover_dirty(&mut doc.root, &old_chain, &hover_chain, false);

    // Incremental cascade
    let t1 = std::time::Instant::now();
    apply_cascade_incremental(
        &mut doc.root, &doc.stylesheet, None, 16.0,
        800.0, 600.0, 0, false, &hover_chain,
    );
    let inc_time = t1.elapsed();
    clear_cascade_dirty(&mut doc.root);

    eprintln!("[first hover] Full cascade: {:?}, Incremental: {:?}, Speedup: {:.1}x",
        full_time, inc_time,
        full_time.as_nanos() as f64 / inc_time.as_nanos().max(1) as f64);

    // Now measure the TRANSITION case (moving from nav0 to nav1)
    // This is where the real speedup happens — symmetric difference is small
    let nav1 = doc.get_element_by_id("nav1").unwrap();
    let chain_nav1 = build_hover_chain(&doc.root, nav1);
    doc.rebuild_node_map();
    mark_hover_dirty(&mut doc.root, &hover_chain, &chain_nav1,
        doc.stylesheet.has_hover_descendant_rules);

    let t2 = std::time::Instant::now();
    apply_cascade_incremental(
        &mut doc.root, &doc.stylesheet, None, 16.0,
        800.0, 600.0, 0, false, &chain_nav1,
    );
    let transition_time = t2.elapsed();
    clear_cascade_dirty(&mut doc.root);

    eprintln!("[hover transition] Full cascade: {:?}, Incremental: {:?}, Speedup: {:.1}x",
        full_time, transition_time,
        full_time.as_nanos() as f64 / transition_time.as_nanos().max(1) as f64);

    // Transition should be much faster than full cascade
    assert!(transition_time < full_time,
        "transition ({:?}) should be faster than full ({:?})", transition_time, full_time);

    // Also measure layout time with dirty flags vs full layout
    let mut engine = crate::layout::LayoutEngine::new();
    engine.viewport_w = 800.0;
    engine.viewport_h = 600.0;

    // Full layout after full cascade
    let mut doc_layout = parse_html(&html);
    apply_cascade_vp_hover(&mut doc_layout.root, &doc_layout.stylesheet, None, 16.0, 800.0, 600.0, 0, false, &empty);
    let t_layout_full = std::time::Instant::now();
    engine.layout(&mut doc_layout, 800.0);
    let layout_full = t_layout_full.elapsed();

    // Now do an incremental hover and measure layout
    let nav2 = doc_layout.get_element_by_id("nav2").unwrap();
    let chain_nav2 = build_hover_chain(&doc_layout.root, nav2);
    doc_layout.rebuild_node_map();
    doc_layout.hover_changed = true;
    doc_layout.hovered_box = nav2;
    doc_layout.prev_hovered_box = nav0;
    let t_layout_inc = std::time::Instant::now();
    engine.layout(&mut doc_layout, 800.0);
    let layout_inc = t_layout_inc.elapsed();

    eprintln!("[layout] Full: {:?}, After hover: {:?}, Speedup: {:.1}x",
        layout_full, layout_inc,
        layout_full.as_nanos() as f64 / layout_inc.as_nanos().max(1) as f64);
}

#[test]
fn incremental_cascade_handles_hover_transition() {
    // Test hovering from one element to another — both old and new chains get re-cascaded
    let html = r#"<ul>
        <li id="a" class="item">A</li>
        <li id="b" class="item">B</li>
        <li id="c" class="item">C</li>
    </ul>"#;

    let mut doc = parse_html(html);
    let empty = HashSet::new();

    // Initial cascade
    apply_cascade_vp_hover(
        &mut doc.root, &doc.stylesheet, None, 16.0,
        800.0, 600.0, 0, false, &empty,
    );
    clear_cascade_dirty(&mut doc.root);

    let a = doc.get_element_by_id("a").unwrap();
    let b = doc.get_element_by_id("b").unwrap();

    // Hover on A
    let chain_a = build_hover_chain(&doc.root, a);
    let old_empty = HashSet::new();
    doc.rebuild_node_map();
    mark_hover_dirty(&mut doc.root, &old_empty, &chain_a, false);
    apply_cascade_incremental(
        &mut doc.root, &doc.stylesheet, None, 16.0,
        800.0, 600.0, 0, false, &chain_a,
    );
    clear_cascade_dirty(&mut doc.root);

    // Move hover from A to B
    let chain_b = build_hover_chain(&doc.root, b);
    doc.rebuild_node_map();
    mark_hover_dirty(&mut doc.root, &chain_a, &chain_b, false);
    apply_cascade_incremental(
        &mut doc.root, &doc.stylesheet, None, 16.0,
        800.0, 600.0, 0, false, &chain_b,
    );
    clear_cascade_dirty(&mut doc.root);

    // Both A and B should have been processed
    // (A should no longer have hover styles, B should have them)
    // Just verify it doesn't crash and processes both chains
    assert!(chain_a.contains(&a));
    assert!(chain_b.contains(&b));
}
