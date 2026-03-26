#[cfg(test)]
mod tests {
    use crate::tests::harness::parse_and_layout;
    use crate::layout::hit_test::hit_test_box_at;

    #[test]
    fn test_float_right_on_same_line() {
        let html = r#"
            <style>body { margin: 0; }</style>
            <div style="width: 200px; font-size: 10px;">
                Text <span id="float" style="float: right; width: 50px; height: 10px;"></span>
            </div>
        "#;
        let doc = parse_and_layout(html, 200.0);

        let float_box = crate::tests::test_grid::find_by_id(&doc.root, "float").expect("float box not found");
        // Float should be at Y=0 (same as text)
        assert_eq!(float_box.layout.border_rect.y, 0.0, "Float should be on the same line as text");
        assert_eq!(float_box.layout.border_rect.x, 150.0, "Float should be on the right edge");
    }

    #[test]
    fn test_float_left_no_overlap() {
        let html = r#"
            <div id="wrap" style="width: 200px; font-size: 10px;">
                <span id="dot" style="float: left; width: 10px; height: 10px;"></span>
                <span id="text">Text</span>
            </div>
        "#;
        let doc = parse_and_layout(html, 200.0);

        let dot_box  = crate::tests::test_grid::find_by_id(&doc.root, "dot") .expect("dot box not found");
        let wrap_box = crate::tests::test_grid::find_by_id(&doc.root, "wrap").expect("wrap box not found");

        println!("Dot rect: {:?}", dot_box.layout.border_rect);

        // The float occupies x=0..10 on the first line.
        // The parent's first line_cache entry must start at x >= 10 so
        // the inline text doesn't overlap the float.
        assert!(
            !wrap_box.layout.line_cache.is_empty(),
            "wrap div must have at least one layout line"
        );
        let first_line = &wrap_box.layout.line_cache[0];
        println!("First line x: {}", first_line.x);
        assert!(
            first_line.x >= 10.0,
            "First text line must start after the 10px float (x >= 10); got x={}",
            first_line.x
        );
    }

    #[test]
    fn test_float_sibling_no_leakage() {
        let html = r#"
            <div style="width: 200px; font-size: 10px;">
                <div id="row1" style="height: 10px;">
                    <span style="float: right; width: 50px; height: 10px;"></span>
                    Row1
                </div>
                <div id="row2" style="height: 10px;">
                    Row2
                </div>
            </div>
        "#;
        let doc = parse_and_layout(html, 200.0);
        
        let row2_box = crate::tests::test_grid::find_by_id(&doc.root, "row2").expect("row2 not found");
        
        let row2_text_line = &row2_box.layout.line_cache[0];
        // Row2 should start at its box's content_rect.x
        assert!((row2_text_line.x - row2_box.layout.content_rect.x).abs() < 0.1, 
                "Row2.x ({}) should be equal to content_rect.x ({})", 
                row2_text_line.x, row2_box.layout.content_rect.x);
    }
}
