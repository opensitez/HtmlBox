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
    assert!(c1.border_rect.w > c2.border_rect.w);
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
    assert_eq!(c1.border_rect.w, 100.0);
    assert_eq!(c2.border_rect.w, 100.0);
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
    assert_eq!(c1.border_rect.w, 100.0);
    assert_eq!(c2.border_rect.w, 300.0);
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

    assert!((a.border_rect.w - 100.0).abs() < 2.0,
        "subgrid child a width should be 100px, got {}", a.border_rect.w);
    assert!((b.border_rect.w - 100.0).abs() < 2.0,
        "subgrid child b width should be 100px, got {}", b.border_rect.w);
    assert!((c.border_rect.w - 100.0).abs() < 2.0,
        "subgrid child c width should be 100px, got {}", c.border_rect.w);

    // Children should be horizontally sequential
    assert!(b.border_rect.x > a.border_rect.x, "b should be right of a");
    assert!(c.border_rect.x > b.border_rect.x, "c should be right of b");
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

    assert!((a.border_rect.w - 50.0).abs()  < 2.0, "a width={}", a.border_rect.w);
    assert!((b.border_rect.w - 150.0).abs() < 2.0, "b width={}", b.border_rect.w);
    assert!((c.border_rect.w - 100.0).abs() < 2.0, "c width={}", c.border_rect.w);
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
    assert!((a.border_rect.w - 100.0).abs() < 2.0, "a width={}", a.border_rect.w);
    assert!((b.border_rect.w - 100.0).abs() < 2.0, "b width={}", b.border_rect.w);
    // a and b should be in the same row, b to the right of a
    assert!(b.border_rect.x > a.border_rect.x, "b must be right of a");
    assert!((a.border_rect.y - b.border_rect.y).abs() < 2.0, "a and b same row");
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

    assert!((a.border_rect.w -  48.0).abs() < 2.0, "a={}", a.border_rect.w);
    assert!((b.border_rect.w - 200.0).abs() < 2.0, "b={}", b.border_rect.w);
    assert!((c.border_rect.w - 100.0).abs() < 2.0, "c={}", c.border_rect.w);
    assert!((d.border_rect.w -  80.0).abs() < 2.0, "d={}", d.border_rect.w);

    assert!((a.border_rect.x -   0.0).abs() < 2.0, "ax={}", a.border_rect.x);
    assert!((b.border_rect.x -  48.0).abs() < 2.0, "bx={}", b.border_rect.x);
    assert!((c.border_rect.x - 248.0).abs() < 2.0, "cx={}", c.border_rect.x);
    assert!((d.border_rect.x - 348.0).abs() < 2.0, "dx={}", d.border_rect.x);
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
    assert!((r1a.border_rect.x - 0.0).abs() < 2.0, "r1a.x={}", r1a.border_rect.x);
    assert!((r2a.border_rect.x - 0.0).abs() < 2.0, "r2a.x={}", r2a.border_rect.x);
    // Both rows' second cells should be at x=100
    assert!((r1b.border_rect.x - 100.0).abs() < 2.0, "r1b.x={}", r1b.border_rect.x);
    assert!((r2b.border_rect.x - 100.0).abs() < 2.0, "r2b.x={}", r2b.border_rect.x);
    // Row 2 should be below row 1
    assert!(r2a.border_rect.y > r1a.border_rect.y, "r2 must be below r1");
}
