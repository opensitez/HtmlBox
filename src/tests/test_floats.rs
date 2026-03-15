#[cfg(test)]
mod tests {
    use crate::tests::harness::parse_and_layout;
    use crate::layout::hit_test::hit_test_box_at;

    #[test]
    fn test_float_right_on_same_line() {
        let html = r#"
            <div style="width: 200px; font-size: 10px;">
                Text <span id="float" style="float: right; width: 50px; height: 10px;"></span>
            </div>
        "#;
        let doc = parse_and_layout(html, 200.0);
        
        let float_box = crate::tests::test_grid::find_by_id(&doc.root, "float").expect("float box not found");
        // Float should be at Y=0 (same as text)
        assert_eq!(float_box.border_rect.y, 0.0, "Float should be on the same line as text");
        assert_eq!(float_box.border_rect.x, 150.0, "Float should be on the right edge");
    }

    #[test]
    fn test_float_left_no_overlap() {
        let html = r#"
            <div style="width: 200px; font-size: 10px;">
                <span id="dot" style="float: left; width: 10px; height: 10px;"></span>
                <span id="text">Text</span>
            </div>
        "#;
        let doc = parse_and_layout(html, 200.0);
        
        let dot_box = crate::tests::test_grid::find_by_id(&doc.root, "dot").expect("dot box not found");
        let text_box = crate::tests::test_grid::find_by_id(&doc.root, "text").expect("text box not found");
        
        println!("Dot rect: {:?}", dot_box.border_rect);
        println!("Text rect: {:?}", text_box.border_rect);

        // Text should start after the dot (x >= 10)
        assert!(text_box.border_rect.x >= 10.0, "Text should not overlap float: left");
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
        
        let row2_text_line = &row2_box.line_cache[0];
        // Row2 should start at its box's content_rect.x
        assert!((row2_text_line.x - row2_box.content_rect.x).abs() < 0.1, 
                "Row2.x ({}) should be equal to content_rect.x ({})", 
                row2_text_line.x, row2_box.content_rect.x);
    }
}
