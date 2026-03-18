#[test]
fn fixed_center_plus() {
    use rhtmledit::load_html;
    // Use actual demo.html HTML — newline before the +
    let doc = load_html(
        "<div style=\"position: fixed; bottom: 16px; right: 16px; width: 48px; height: 48px; background-color: #3498db; color: white; text-align: center; font-size: 24pt; font-weight: bold; z-index: 100;\">\n+</div>",
        800.0);
    
    fn find<'a>(node: &'a rhtmledit::HtmlBox, pred: &dyn Fn(&rhtmledit::HtmlBox) -> bool) -> Option<&'a rhtmledit::HtmlBox> {
        if pred(node) { return Some(node); }
        for c in &node.children { if let Some(b) = find(c, pred) { return Some(b); } }
        None
    }
    
    let btn = find(&doc.root, &|b| b.attributes.get("style").map(|s| s.contains("3498db")).unwrap_or(false)).unwrap();
    println!("border_rect: {:?}", btn.border_rect);
    println!("content_rect: {:?}", btn.content_rect);
    println!("text_align: {:?}", btn.style.text_align);
    let flat = rhtmledit::dom::get_text_content(btn);
    println!("flat text: {:?}", flat);
    println!("num lines: {}", btn.line_cache.len());
    println!("num inline_runs: {}", btn.inline_runs.len());
    for (i, line) in btn.line_cache.iter().enumerate() {
        let ts = line.text_start.min(flat.len());
        let te = (line.text_start + line.text_length).min(flat.len());
        let text = &flat[ts..te];
        let offset = line.x - btn.content_rect.x;
        let expected = (btn.content_rect.w - line.width) / 2.0;
        println!("line[{}] x={:.2} width={:.2} text={:?} offset={:.2} expected={:.2}",
            i, line.x, line.width, text, offset, expected);
    }
}
