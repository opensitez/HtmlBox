use crate::tests::harness::{parse_and_layout, find_box};
use crate::types::*;

pub fn find_by_id<'a>(node: &'a HtmlBox, id: &str) -> Option<&'a HtmlBox> {
    find_box(node, &|b| b.attributes.get("id").map(|s| s == id).unwrap_or(false))
}

#[test]
fn test_grid_auto_tracks() {
    let html = r#"
        <div style="display: grid; grid-template-columns: auto auto; width: 400px; font-size: 16px;">
            <div id="c1" style="background: red;">Longer Text Content</div>
            <div id="c2" style="background: blue;">Short</div>
        </div>
    "#;
    let doc = parse_and_layout(html, 400.0);
    let c1 = find_by_id(&doc.root, "c1").unwrap();
    let c2 = find_by_id(&doc.root, "c2").unwrap();

    // With auto auto, items should take their content width.
    // "Longer Text Content" is ~19 chars. "Short" is 5 chars.
    // In current buggy implementation, they might get 200px each (equal share).
    assert!(c1.layout.border_rect.w > c2.layout.border_rect.w);
}

#[test]
fn test_grid_item_stretch_background() {
    let html = r#"
        <div style="display: grid; grid-template-columns: 100px 100px; width: 200px; font-size: 16px;">
            <div id="c1" style="background: red; display: inline-block;">Small</div>
            <div id="c2" style="background: blue;">Block</div>
        </div>
    "#;
    let doc = parse_and_layout(html, 200.0);
    let c1 = find_by_id(&doc.root, "c1").unwrap();
    let c2 = find_by_id(&doc.root, "c2").unwrap();

    // Default justify-self is stretch.
    // c1 is inline-block, so it might shrink to fit if not forced to stretch.
    // c2 is block, so it should stretch naturally.
    assert_eq!(c1.layout.border_rect.w, 100.0);
    assert_eq!(c2.layout.border_rect.w, 100.0);
}

#[test]
fn test_grid_fr_with_auto() {
    let html = r#"
        <div style="display: grid; grid-template-columns: auto 1fr; width: 400px; gap: 0;">
            <div id="c1" style="width: 100px;">Fixed</div>
            <div id="c2">Flexible</div>
        </div>
    "#;
    let doc = parse_and_layout(html, 400.0);
    let c1 = find_by_id(&doc.root, "c1").unwrap();
    let c2 = find_by_id(&doc.root, "c2").unwrap();

    // c1 should be 100px, c2 should be 300px.
    // In current buggy implementation, if there is an 'fr', 'auto' gets 0.
    assert_eq!(c1.layout.border_rect.w, 100.0);
    assert_eq!(c2.layout.border_rect.w, 300.0);
}

// ── Subgrid tests ─────────────────────────────────────────────────────────────

#[test]
fn subgrid_columns_parsed() {
    use crate::css::parse_track_list;
    let mut dummy = Vec::new();
    let tracks = parse_track_list("subgrid", &mut dummy);
    assert_eq!(tracks.len(), 1);
    assert!(tracks[0].is_subgrid(), "subgrid keyword should produce a Subgrid sentinel");
}

#[test]
fn subgrid_columns_flag_set_in_style() {
    let doc = parse_and_layout(
        r#"<html><body>
          <div style="display:grid; grid-template-columns: 100px 200px; width:400px">
            <div id="sub" style="display:grid; grid-template-columns:subgrid; grid-column:1/3">
              <div id="a">A</div>
              <div id="b">B</div>
            </div>
          </div>
        </body></html>"#,
        400.0,
    );
    let sub = find_by_id(&doc.root, "sub").expect("sub");
    assert!(sub.style.subgrid_columns, "subgrid_columns flag must be set");
    assert!(!sub.style.subgrid_rows, "subgrid_rows must not be set");
}

#[test]
fn subgrid_children_align_to_parent_tracks() {
    // Parent: 3 columns of 100px each. Subgrid spans all 3 and also has 3 children.
    // Each child should be 100px wide (inheriting parent track sizes).
    let doc = parse_and_layout(
        r#"<html><body style="margin:0">
          <div id="parent" style="display:grid; grid-template-columns:100px 100px 100px;
                                  column-gap:0; width:300px; margin:0">
            <div id="sub" style="display:grid; grid-template-columns:subgrid;
                                  grid-column:1/4; margin:0; padding:0">
              <div id="a" style="margin:0">A</div>
              <div id="b" style="margin:0">B</div>
              <div id="c" style="margin:0">C</div>
            </div>
          </div>
        </body></html>"#,
        400.0,
    );
    let a = find_by_id(&doc.root, "a").expect("a");
    let b = find_by_id(&doc.root, "b").expect("b");
    let c = find_by_id(&doc.root, "c").expect("c");

    assert!((a.layout.border_rect.w - 100.0).abs() < 2.0,
        "subgrid child a width should be 100px, got {}", a.layout.border_rect.w);
    assert!((b.layout.border_rect.w - 100.0).abs() < 2.0,
        "subgrid child b width should be 100px, got {}", b.layout.border_rect.w);
    assert!((c.layout.border_rect.w - 100.0).abs() < 2.0,
        "subgrid child c width should be 100px, got {}", c.layout.border_rect.w);

    // Children should be horizontally sequential
    assert!(b.layout.border_rect.x > a.layout.border_rect.x, "b should be right of a");
    assert!(c.layout.border_rect.x > b.layout.border_rect.x, "c should be right of b");
}

#[test]
fn subgrid_children_x_positions_match_parent_tracks() {
    // Parent columns: 50px, 150px, 100px with 0 gap.
    // Subgrid spans columns 1-3, its children should align at x=0, x=50, x=200.
    let doc = parse_and_layout(
        r#"<html><body style="margin:0">
          <div id="parent" style="display:grid; grid-template-columns:50px 150px 100px;
                                  column-gap:0; width:300px; margin:0">
            <div id="sub" style="display:grid; grid-template-columns:subgrid;
                                  grid-column:1/4; margin:0; padding:0">
              <div id="a" style="margin:0">A</div>
              <div id="b" style="margin:0">B</div>
              <div id="c" style="margin:0">C</div>
            </div>
          </div>
        </body></html>"#,
        400.0,
    );
    let a = find_by_id(&doc.root, "a").expect("a");
    let b = find_by_id(&doc.root, "b").expect("b");
    let c = find_by_id(&doc.root, "c").expect("c");

    assert!((a.layout.border_rect.w - 50.0).abs()  < 2.0, "a width={}", a.layout.border_rect.w);
    assert!((b.layout.border_rect.w - 150.0).abs() < 2.0, "b width={}", b.layout.border_rect.w);
    assert!((c.layout.border_rect.w - 100.0).abs() < 2.0, "c width={}", c.layout.border_rect.w);
}

#[test]
fn subgrid_rows_flag_set_in_style() {
    let doc = parse_and_layout(
        r#"<html><body>
          <div style="display:grid; grid-template-rows:50px 50px; width:200px">
            <div id="sub" style="display:grid; grid-template-rows:subgrid; grid-row:1/3;
                                  grid-template-columns:1fr">
              <div id="x">X</div>
              <div id="y">Y</div>
            </div>
          </div>
        </body></html>"#,
        400.0,
    );
    let sub = find_by_id(&doc.root, "sub").expect("sub");
    assert!(sub.style.subgrid_rows, "subgrid_rows flag must be set");
}

#[test]
fn subgrid_both_axes() {
    // Subgrid can inherit both column and row tracks simultaneously.
    let doc = parse_and_layout(
        r#"<html><body style="margin:0">
          <div id="parent" style="display:grid;
                                  grid-template-columns:100px 100px;
                                  grid-template-rows:80px 80px;
                                  column-gap:0; row-gap:0; width:200px; margin:0">
            <div id="sub" style="display:grid;
                                  grid-template-columns:subgrid;
                                  grid-template-rows:subgrid;
                                  grid-column:1/3; grid-row:1/3;
                                  margin:0; padding:0">
              <div id="a" style="margin:0">A</div>
              <div id="b" style="margin:0">B</div>
              <div id="c" style="margin:0">C</div>
              <div id="d" style="margin:0">D</div>
            </div>
          </div>
        </body></html>"#,
        400.0,
    );
    let sub = find_by_id(&doc.root, "sub").expect("sub");
    assert!(sub.style.subgrid_columns, "subgrid_columns must be set");
    assert!(sub.style.subgrid_rows,    "subgrid_rows must be set");

    let a = find_by_id(&doc.root, "a").expect("a");
    let b = find_by_id(&doc.root, "b").expect("b");
    assert!((a.layout.border_rect.w - 100.0).abs() < 2.0, "a width={}", a.layout.border_rect.w);
    assert!((b.layout.border_rect.w - 100.0).abs() < 2.0, "b width={}", b.layout.border_rect.w);
    // a and b should be in the same row, b to the right of a
    assert!(b.layout.border_rect.x > a.layout.border_rect.x, "b must be right of a");
    assert!((a.layout.border_rect.y - b.layout.border_rect.y).abs() < 2.0, "a and b same row");
}

#[test]
fn subgrid_row_list_pattern() {
    // Classic "table via subgrid" pattern:
    // Parent: 4 fixed columns. Each row is a subgrid spanning all 4.
    let doc = parse_and_layout(
        r#"<html><body style="margin:0">
          <div id="parent" style="display:grid;
                grid-template-columns:48px 200px 100px 80px;
                column-gap:0; width:428px; margin:0">
            <div id="row1" style="display:grid; grid-template-columns:subgrid;
                                  grid-column:1/5; margin:0; padding:0">
              <div id="a">A</div>
              <div id="b">B</div>
              <div id="c">C</div>
              <div id="d">D</div>
            </div>
          </div>
        </body></html>"#,
        800.0,
    );
    let a = find_by_id(&doc.root, "a").expect("a");
    let b = find_by_id(&doc.root, "b").expect("b");
    let c = find_by_id(&doc.root, "c").expect("c");
    let d = find_by_id(&doc.root, "d").expect("d");

    assert!((a.layout.border_rect.w -  48.0).abs() < 2.0, "a={}", a.layout.border_rect.w);
    assert!((b.layout.border_rect.w - 200.0).abs() < 2.0, "b={}", b.layout.border_rect.w);
    assert!((c.layout.border_rect.w - 100.0).abs() < 2.0, "c={}", c.layout.border_rect.w);
    assert!((d.layout.border_rect.w -  80.0).abs() < 2.0, "d={}", d.layout.border_rect.w);

    assert!((a.layout.border_rect.x -   0.0).abs() < 2.0, "ax={}", a.layout.border_rect.x);
    assert!((b.layout.border_rect.x -  48.0).abs() < 2.0, "bx={}", b.layout.border_rect.x);
    assert!((c.layout.border_rect.x - 248.0).abs() < 2.0, "cx={}", c.layout.border_rect.x);
    assert!((d.layout.border_rect.x - 348.0).abs() < 2.0, "dx={}", d.layout.border_rect.x);
}

#[test]
fn subgrid_multi_row_alignment() {
    // Two subgrid rows both using the same 3 parent columns.
    // All cells in column 0 should share the same x=0; column 1 → x=100; column 2 → x=260.
    let doc = parse_and_layout(
        r#"<html><body style="margin:0">
          <div id="parent" style="display:grid;
                grid-template-columns:100px 160px 80px;
                column-gap:0; width:340px; margin:0">
            <div id="r1" style="display:grid; grid-template-columns:subgrid;
                                grid-column:1/4; margin:0; padding:0">
              <div id="r1a">R1A</div><div id="r1b">R1B</div><div id="r1c">R1C</div>
            </div>
            <div id="r2" style="display:grid; grid-template-columns:subgrid;
                                grid-column:1/4; margin:0; padding:0">
              <div id="r2a">R2A</div><div id="r2b">R2B</div><div id="r2c">R2C</div>
            </div>
          </div>
        </body></html>"#,
        800.0,
    );
    let r1a = find_by_id(&doc.root, "r1a").expect("r1a");
    let r1b = find_by_id(&doc.root, "r1b").expect("r1b");
    let r2a = find_by_id(&doc.root, "r2a").expect("r2a");
    let r2b = find_by_id(&doc.root, "r2b").expect("r2b");

    // Both rows' first cells should be at x=0
    assert!((r1a.layout.border_rect.x - 0.0).abs() < 2.0, "r1a.x={}", r1a.layout.border_rect.x);
    assert!((r2a.layout.border_rect.x - 0.0).abs() < 2.0, "r2a.x={}", r2a.layout.border_rect.x);
    // Both rows' second cells should be at x=100
    assert!((r1b.layout.border_rect.x - 100.0).abs() < 2.0, "r1b.x={}", r1b.layout.border_rect.x);
    assert!((r2b.layout.border_rect.x - 100.0).abs() < 2.0, "r2b.x={}", r2b.layout.border_rect.x);
    // Row 2 should be below row 1
    assert!(r2a.layout.border_rect.y > r1a.layout.border_rect.y, "r2 must be below r1");
}

// ── Placement algorithm regression tests ─────────────────────────────────────

#[test]
fn span_only_column_is_not_explicitly_placed() {
    // `grid-column: span 1` must NOT count as an explicit column placement.
    // Without this fix all items were treated as explicitly placed and stacked
    // on top of each other at (col=0, row=0).
    // Two items with `grid-column: span 1` in a 2-column grid should end up
    // side-by-side (different x positions), not overlapping.
    let doc = parse_and_layout(
        r#"<html><body style="margin:0">
          <div style="display:grid; grid-template-columns:100px 100px;
                      column-gap:0; width:200px; margin:0">
            <div id="a" style="grid-column:span 1">A</div>
            <div id="b" style="grid-column:span 1">B</div>
          </div>
        </body></html>"#,
        800.0,
    );
    let a = find_by_id(&doc.root, "a").expect("a");
    let b = find_by_id(&doc.root, "b").expect("b");
    assert!((a.layout.border_rect.x -   0.0).abs() < 2.0, "a.x={}", a.layout.border_rect.x);
    assert!((b.layout.border_rect.x - 100.0).abs() < 2.0, "b.x={}", b.layout.border_rect.x);
    // Same row
    assert!((a.layout.border_rect.y - b.layout.border_rect.y).abs() < 2.0, "must be same row");
}

#[test]
fn span_col_with_four_items_wraps_to_two_rows() {
    // Four items each `grid-column: span 1` in a 2-column grid.
    // Before the fix all four were placed at (0,0) and only the last was visible.
    // After the fix: items 1&2 in row 0, items 3&4 in row 1.
    let doc = parse_and_layout(
        r#"<html><body style="margin:0">
          <div style="display:grid; grid-template-columns:100px 100px;
                      column-gap:0; width:200px; margin:0">
            <div id="a" style="grid-column:span 1; height:50px">A</div>
            <div id="b" style="grid-column:span 1; height:50px">B</div>
            <div id="c" style="grid-column:span 1; height:50px">C</div>
            <div id="d" style="grid-column:span 1; height:50px">D</div>
          </div>
        </body></html>"#,
        800.0,
    );
    let a = find_by_id(&doc.root, "a").expect("a");
    let b = find_by_id(&doc.root, "b").expect("b");
    let c = find_by_id(&doc.root, "c").expect("c");
    let d = find_by_id(&doc.root, "d").expect("d");
    // Row 0: a at x=0, b at x=100
    assert!((a.layout.border_rect.x -   0.0).abs() < 2.0, "a.x={}", a.layout.border_rect.x);
    assert!((b.layout.border_rect.x - 100.0).abs() < 2.0, "b.x={}", b.layout.border_rect.x);
    assert!((a.layout.border_rect.y - b.layout.border_rect.y).abs() < 2.0, "a,b same row");
    // Row 1: c at x=0, d at x=100, both below row 0
    assert!((c.layout.border_rect.x -   0.0).abs() < 2.0, "c.x={}", c.layout.border_rect.x);
    assert!((d.layout.border_rect.x - 100.0).abs() < 2.0, "d.x={}", d.layout.border_rect.x);
    assert!(c.layout.border_rect.y > a.layout.border_rect.y, "c must be below a");
    assert!((c.layout.border_rect.y - d.layout.border_rect.y).abs() < 2.0, "c,d same row");
}

#[test]
fn row_locked_auto_column_step2_placement() {
    // CSS Grid spec §8.5 step 2: items with a definite row (`grid-row: 1/2`)
    // but auto column should be locked to that row and auto-placed only in the
    // column axis — creating implicit columns if the explicit ones are full.
    // Two items with `grid-row: 1/2; grid-column: span 1` in a 1-column grid:
    // item 1 → col 1 (explicit), item 2 → col 2 (implicit).
    // Before the fix both were treated as explicitly placed and stacked at (0,0).
    let doc = parse_and_layout(
        r#"<html><body style="margin:0">
          <div style="display:grid; grid-template-columns:100px;
                      column-gap:0; width:300px; margin:0">
            <div id="a" style="grid-row:1/2; grid-column:span 1; height:40px">A</div>
            <div id="b" style="grid-row:1/2; grid-column:span 1; height:40px">B</div>
          </div>
        </body></html>"#,
        800.0,
    );
    let a = find_by_id(&doc.root, "a").expect("a");
    let b = find_by_id(&doc.root, "b").expect("b");
    // Both locked to row 0 → same y
    assert!((a.layout.border_rect.y - b.layout.border_rect.y).abs() < 2.0,
        "both must be in the same row; a.y={} b.y={}", a.layout.border_rect.y, b.layout.border_rect.y);
    // Must be in different columns → different x
    assert!((b.layout.border_rect.x - a.layout.border_rect.x).abs() > 50.0,
        "must be in different columns; a.x={} b.x={}", a.layout.border_rect.x, b.layout.border_rect.x);
}

// ─── col-span-full (grid-column: 1 / -1) in grids and subgrids ──────────────

#[test]
fn grid_col_span_full_spans_all_columns() {
    // grid-column: 1 / -1 should span all explicit columns
    let html = r#"
        <div style="display:grid; grid-template-columns: repeat(6, 1fr); width:600px; gap:0;">
            <div id="full" style="grid-column: 1 / -1; height:20px;"></div>
            <div id="one" style="height:20px;"></div>
        </div>
    "#;
    let doc = parse_and_layout(html, 600.0);
    let full = find_by_id(&doc.root, "full").unwrap();
    assert!(full.layout.content_rect.w > 500.0,
        "col-span-full must span all 6 columns, got width={}", full.layout.content_rect.w);
}

#[test]
fn subgrid_col_span_full_spans_inherited_columns() {
    // A subgrid child with grid-column: 1 / -1 should span ALL inherited columns,
    // not just 1.
    let html = r#"
        <div style="display:grid; grid-template-columns: repeat(6, 1fr); width:600px; gap:0;">
            <div id="sub" style="grid-column: 1 / -1; display:grid; grid-template-columns: subgrid;">
                <div id="inner-full" style="grid-column: 1 / -1; height:30px;"></div>
            </div>
        </div>
    "#;
    let doc = parse_and_layout(html, 600.0);
    let sub = find_by_id(&doc.root, "sub").unwrap();
    let inner = find_by_id(&doc.root, "inner-full").unwrap();
    assert!(sub.layout.content_rect.w > 500.0,
        "subgrid must span all 6 columns, got width={}", sub.layout.content_rect.w);
    assert!(inner.layout.content_rect.w > 500.0,
        "inner col-span-full in subgrid must span all inherited columns, got width={}", inner.layout.content_rect.w);
}

#[test]
fn subgrid_col_span_full_with_sibling() {
    // In a subgrid spanning 7 of 12 columns, a child with col-span-full
    // should span all 7 inherited columns, not just 1.
    let html = r#"
        <div style="display:grid; grid-template-columns: repeat(12, 1fr); width:1200px; gap:0;">
            <div style="grid-column: span 5; height:50px;"></div>
            <div id="sub" style="grid-column: span 7; display:grid; grid-template-columns: subgrid;">
                <div id="first" style="grid-column: span 3; height:30px;"></div>
                <div id="rest" style="grid-column: 1 / -1; height:30px;"></div>
            </div>
        </div>
    "#;
    let doc = parse_and_layout(html, 1200.0);
    let sub = find_by_id(&doc.root, "sub").unwrap();
    let rest = find_by_id(&doc.root, "rest").unwrap();
    // sub spans 7 columns = 700px
    assert!(sub.layout.content_rect.w > 600.0,
        "subgrid spanning 7 columns should be ~700px, got {}", sub.layout.content_rect.w);
    // rest with col-span-full should also span all 7 inherited columns
    assert!(rest.layout.content_rect.w > 600.0,
        "col-span-full in subgrid must span all 7 inherited columns, got {}", rest.layout.content_rect.w);
}

#[test]
fn subgrid_column_locked_item_uses_explicit_columns() {
    // Item with explicit grid-column-start but auto row should be column-locked,
    // not auto-placed with span=1.
    let html = r#"
        <div style="display:grid; grid-template-columns: repeat(6, 1fr); width:600px; gap:0;">
            <div id="sub" style="grid-column: 1 / -1; display:grid; grid-template-columns: subgrid;">
                <div id="a" style="grid-column: span 2; height:30px;"></div>
                <div id="b" style="grid-column: 3 / 6; height:30px;"></div>
            </div>
        </div>
    "#;
    let doc = parse_and_layout(html, 600.0);
    let a = find_by_id(&doc.root, "a").unwrap();
    let b = find_by_id(&doc.root, "b").unwrap();
    // a: spans 2 columns = 200px
    assert!(a.layout.content_rect.w > 150.0,
        "a spanning 2 columns should be ~200px, got {}", a.layout.content_rect.w);
    // b: columns 3..6 = 3 columns = 300px
    assert!(b.layout.content_rect.w > 250.0,
        "b spanning columns 3-6 should be ~300px, got {}", b.layout.content_rect.w);
}

#[test]
fn nested_subgrid_col_span_full() {
    // Two levels of subgrid, innermost has col-span-full
    let html = r#"
        <div style="display:grid; grid-template-columns: repeat(12, 1fr); width:1200px; gap:0;">
            <div id="outer-sub" style="grid-column: span 6; display:grid; grid-template-columns: subgrid;">
                <div id="inner-sub" style="grid-column: 1 / -1; display:grid; grid-template-columns: subgrid;">
                    <div id="deep" style="grid-column: 1 / -1; height:20px;"></div>
                </div>
            </div>
        </div>
    "#;
    let doc = parse_and_layout(html, 1200.0);
    let outer = find_by_id(&doc.root, "outer-sub").unwrap();
    let inner = find_by_id(&doc.root, "inner-sub").unwrap();
    let deep = find_by_id(&doc.root, "deep").unwrap();
    // Each should span 6 columns = 600px
    assert!(outer.layout.content_rect.w > 500.0,
        "outer subgrid must be ~600px, got {}", outer.layout.content_rect.w);
    assert!(inner.layout.content_rect.w > 500.0,
        "inner subgrid with col-span-full must be ~600px, got {}", inner.layout.content_rect.w);
    assert!(deep.layout.content_rect.w > 500.0,
        "deep col-span-full in nested subgrid must be ~600px, got {}", deep.layout.content_rect.w);
}

#[test]
fn aol_trending_layout_pattern() {
    // Mimics the netscape.com trending section:
    // 12-col grid > [span-5] [span-7 subgrid > [span-3] [col-span-full subgrid]]
    let html = r#"
        <div style="display:grid; grid-template-columns: repeat(12, 1fr); width:1200px; column-gap:40px;">
            <div id="left" style="grid-column: span 5; height:100px;"></div>
            <div id="right" style="grid-column: span 7; display:grid; grid-template-columns: subgrid;">
                <div id="col3" style="grid-column: span 3; height:80px;"></div>
                <div id="articles" style="grid-column: 1 / -1; display:grid; grid-template-columns: subgrid;">
                    <div id="art1" style="grid-column: span 3; height:40px;"></div>
                    <div id="art2" style="grid-column: span 3; height:40px;"></div>
                </div>
            </div>
        </div>
    "#;
    let doc = parse_and_layout(html, 1200.0);
    let right = find_by_id(&doc.root, "right").unwrap();
    let articles = find_by_id(&doc.root, "articles").unwrap();
    let art1 = find_by_id(&doc.root, "art1").unwrap();

    // right spans 7 columns
    assert!(right.layout.content_rect.w > 400.0,
        "right panel (7 cols) too narrow: {}", right.layout.content_rect.w);
    // articles with col-span-full should span all 7 inherited columns
    assert!(articles.layout.content_rect.w > 400.0,
        "articles col-span-full in subgrid too narrow: {} (should match right={})",
        articles.layout.content_rect.w, right.layout.content_rect.w);
    // art1 with span 3 should be ~3/7 of articles width
    assert!(art1.layout.content_rect.w > 100.0,
        "art1 span-3 in nested subgrid too narrow: {}", art1.layout.content_rect.w);
}
