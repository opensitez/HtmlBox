use crate::types::{
    AlignItems, BorderStyle, Color, ComputedStyle, CssLength, Display, Document, Float,
    FlexDirection, FlexWrap, FontStyle, FontWeight, HtmlBox, JustifyContent, Position,
    TextAlign, TextTransform, WhiteSpace,
};
use crate::css::{CssRule, CssSelector, SelectorPart, Combinator, ua_stylesheet};

// ─── Utility ──────────────────────────────────────────────────────────────────

/// Escape `&`, `<`, `>`, and `"` for safe HTML output.
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&'  => out.push_str("&amp;"),
            '<'  => out.push_str("&lt;"),
            '>'  => out.push_str("&gt;"),
            '"'  => out.push_str("&quot;"),
            _    => out.push(ch),
        }
    }
    out
}

// ─── Length serialization ─────────────────────────────────────────────────────

/// Convert a `CssLength` to its CSS string representation.
/// Returns an empty string when the length should not be emitted.
pub fn serialize_length(len: &CssLength) -> String {
    match len {
        CssLength::Auto    => String::new(),  // auto == default, skip
        CssLength::None    => String::new(),
        CssLength::Zero    => String::new(),
        CssLength::Px(v)   => format!("{}px", *v as i32),
        CssLength::Em(v)   => format!("{}em", v),
        CssLength::Rem(v)  => format!("{}rem", v),
        CssLength::Percent(v) => format!("{}%", v),
        CssLength::Vw(v)      => format!("{}vw", v),
        CssLength::Vh(v)      => format!("{}vh", v),
        CssLength::Calc(c) => {
            let labels = ["%", "px", "em", "rem", "vw", "vh"];
            let parts: Vec<String> = c.iter().zip(labels.iter())
                .filter(|(v, _)| **v != 0.0)
                .map(|(v, u)| format!("{}{}", v, u))
                .collect();
            if parts.is_empty() { "0px".to_string() }
            else { format!("calc({})", parts.join(" + ")) }
        }
    }
}

// ─── Color serialization ──────────────────────────────────────────────────────

fn color_to_css(c: Color) -> String {
    if c.a == 255 {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    } else {
        format!(
            "rgba({},{},{},{:.3})",
            c.r, c.g, c.b,
            c.a as f32 / 255.0
        )
    }
}

// ─── Border side ──────────────────────────────────────────────────────────────

fn serialize_border_side(
    width: &CssLength,
    style: BorderStyle,
    color: Color,
) -> String {
    if style == BorderStyle::None || style == BorderStyle::Hidden {
        return String::new();
    }
    let w = match width {
        CssLength::Px(v) if *v > 0.0 => *v as i32,
        _ => return String::new(),
    };
    let style_str = match style {
        BorderStyle::Solid  => "solid",
        BorderStyle::Dashed => "dashed",
        BorderStyle::Dotted => "dotted",
        BorderStyle::Double => "double",
        BorderStyle::Inset  => "inset",
        BorderStyle::Outset => "outset",
        BorderStyle::Groove => "groove",
        BorderStyle::Ridge  => "ridge",
        _                   => "solid",
    };
    format!("{}px {} {}", w, style_str, color_to_css(color))
}

// ─── Edge (margin / padding) ──────────────────────────────────────────────────

fn serialize_edge(
    name: &str,
    top: &CssLength,
    right: &CssLength,
    bottom: &CssLength,
    left: &CssLength,
    parts: &mut Vec<(String, String)>,
) {
    let t = serialize_length(top);
    let r = serialize_length(right);
    let b = serialize_length(bottom);
    let l = serialize_length(left);

    if t.is_empty() && r.is_empty() && b.is_empty() && l.is_empty() {
        return;
    }

    if !t.is_empty() && t == r && r == b && b == l {
        parts.push((name.to_string(), t));
    } else {
        if !t.is_empty() { parts.push((format!("{}-top",    name), t)); }
        if !r.is_empty() { parts.push((format!("{}-right",  name), r)); }
        if !b.is_empty() { parts.push((format!("{}-bottom", name), b)); }
        if !l.is_empty() { parts.push((format!("{}-left",   name), l)); }
    }
}

// ─── Style → inline CSS ───────────────────────────────────────────────────────

/// Serialize a `ComputedStyle` to an inline CSS string (without the `style=""` wrapper).
/// Only properties that differ from their defaults are emitted.
pub fn serialize_style_to_css(style: &ComputedStyle, _tag: &str) -> String {
    let mut parts: Vec<(String, String)> = Vec::new();

    // ── Position ──────────────────────────────────────────────────────────────
    let pos_str = match style.position {
        Position::Static   => "",
        Position::Relative => "relative",
        Position::Absolute => "absolute",
        Position::Fixed    => "fixed",
        Position::Sticky   => "sticky",
    };
    if !pos_str.is_empty() {
        parts.push(("position".into(), pos_str.into()));
    }

    // ── Float ─────────────────────────────────────────────────────────────────
    let float_str = match style.float {
        Float::None  => "",
        Float::Left  => "left",
        Float::Right => "right",
    };
    if !float_str.is_empty() {
        parts.push(("float".into(), float_str.into()));
    }

    // ── Dimensions ────────────────────────────────────────────────────────────
    if !style.width.is_auto() {
        let s = serialize_length(&style.width);
        if !s.is_empty() { parts.push(("width".into(), s)); }
    }
    if !style.height.is_auto() {
        let s = serialize_length(&style.height);
        if !s.is_empty() { parts.push(("height".into(), s)); }
    }

    // ── Margin ────────────────────────────────────────────────────────────────
    serialize_edge(
        "margin",
        &style.margin_top, &style.margin_right,
        &style.margin_bottom, &style.margin_left,
        &mut parts,
    );

    // ── Padding ───────────────────────────────────────────────────────────────
    serialize_edge(
        "padding",
        &style.padding_top, &style.padding_right,
        &style.padding_bottom, &style.padding_left,
        &mut parts,
    );

    // ── Border ────────────────────────────────────────────────────────────────
    let bt = serialize_border_side(
        &style.border_top_width, style.border_top_style, style.border_top_color);
    let br = serialize_border_side(
        &style.border_right_width, style.border_right_style, style.border_right_color);
    let bb = serialize_border_side(
        &style.border_bottom_width, style.border_bottom_style, style.border_bottom_color);
    let bl = serialize_border_side(
        &style.border_left_width, style.border_left_style, style.border_left_color);

    if !bt.is_empty() && bt == br && br == bb && bb == bl {
        parts.push(("border".into(), bt));
    } else {
        if !bt.is_empty() { parts.push(("border-top".into(),    bt)); }
        if !br.is_empty() { parts.push(("border-right".into(),  br)); }
        if !bb.is_empty() { parts.push(("border-bottom".into(), bb)); }
        if !bl.is_empty() { parts.push(("border-left".into(),   bl)); }
    }

    // ── Background color ──────────────────────────────────────────────────────
    if style.background_color.a > 0 {
        parts.push(("background-color".into(), color_to_css(style.background_color)));
    }

    // ── Text color ────────────────────────────────────────────────────────────
    // Always emit color (simple approach, matches C++ behaviour).
    parts.push(("color".into(), color_to_css(style.color)));

    // ── Text alignment ────────────────────────────────────────────────────────
    let align_str = match style.text_align {
        TextAlign::Left    => "",
        TextAlign::Right   => "right",
        TextAlign::Center  => "center",
        TextAlign::Justify => "justify",
        TextAlign::Start   => "start",
        TextAlign::End     => "end",
    };
    if !align_str.is_empty() {
        parts.push(("text-align".into(), align_str.into()));
    }

    // ── Font weight ───────────────────────────────────────────────────────────
    match style.font_weight {
        FontWeight::Bold        => parts.push(("font-weight".into(), "bold".into())),
        FontWeight::Value(v) if v != 400 => parts.push(("font-weight".into(), v.to_string())),
        _ => {}
    }

    // ── Font style ────────────────────────────────────────────────────────────
    if style.font_style == FontStyle::Italic {
        parts.push(("font-style".into(), "italic".into()));
    } else if style.font_style == FontStyle::Oblique {
        parts.push(("font-style".into(), "oblique".into()));
    }

    // ── Text decoration ───────────────────────────────────────────────────────
    if style.text_decoration.underline || style.text_decoration.overline
        || style.text_decoration.strikethrough
    {
        let mut decorations = Vec::new();
        if style.text_decoration.underline    { decorations.push("underline"); }
        if style.text_decoration.overline     { decorations.push("overline"); }
        if style.text_decoration.strikethrough { decorations.push("line-through"); }
        parts.push(("text-decoration".into(), decorations.join(" ")));
    }

    // ── Text transform ────────────────────────────────────────────────────────
    let tt_str = match style.text_transform {
        TextTransform::None       => "",
        TextTransform::Uppercase  => "uppercase",
        TextTransform::Lowercase  => "lowercase",
        TextTransform::Capitalize => "capitalize",
    };
    if !tt_str.is_empty() {
        parts.push(("text-transform".into(), tt_str.into()));
    }

    // ── White space ───────────────────────────────────────────────────────────
    let ws_str = match style.white_space {
        WhiteSpace::Normal  => "",
        WhiteSpace::Nowrap  => "nowrap",
        WhiteSpace::Pre     => "pre",
        WhiteSpace::PreWrap => "pre-wrap",
        WhiteSpace::PreLine => "pre-line",
    };
    if !ws_str.is_empty() {
        parts.push(("white-space".into(), ws_str.into()));
    }

    // ── Display (flex / grid / inline-* variants) ─────────────────────────────
    let display_str = match style.display {
        Display::Flex        => "flex",
        Display::InlineFlex  => "inline-flex",
        Display::Grid        => "grid",
        Display::InlineGrid  => "inline-grid",
        Display::InlineBlock => "inline-block",
        Display::None        => "none",
        _                    => "",
    };
    if !display_str.is_empty() {
        parts.push(("display".into(), display_str.into()));
    }

    // ── Flex container properties ─────────────────────────────────────────────
    if matches!(style.display, Display::Flex | Display::InlineFlex) {
        let dir_str = match style.flex_direction {
            FlexDirection::Row           => "",
            FlexDirection::RowReverse    => "row-reverse",
            FlexDirection::Column        => "column",
            FlexDirection::ColumnReverse => "column-reverse",
        };
        if !dir_str.is_empty() {
            parts.push(("flex-direction".into(), dir_str.into()));
        }

        if style.flex_wrap == FlexWrap::Wrap {
            parts.push(("flex-wrap".into(), "wrap".into()));
        } else if style.flex_wrap == FlexWrap::WrapReverse {
            parts.push(("flex-wrap".into(), "wrap-reverse".into()));
        }

        let jc_str = match style.justify_content {
            JustifyContent::FlexStart    => "",
            JustifyContent::FlexEnd      => "flex-end",
            JustifyContent::Center       => "center",
            JustifyContent::SpaceBetween => "space-between",
            JustifyContent::SpaceAround  => "space-around",
            JustifyContent::SpaceEvenly  => "space-evenly",
        };
        if !jc_str.is_empty() {
            parts.push(("justify-content".into(), jc_str.into()));
        }

        let ai_str = match style.align_items {
            AlignItems::Stretch   => "",
            AlignItems::FlexStart => "flex-start",
            AlignItems::FlexEnd   => "flex-end",
            AlignItems::Center    => "center",
            AlignItems::Baseline  => "baseline",
        };
        if !ai_str.is_empty() {
            parts.push(("align-items".into(), ai_str.into()));
        }
    }

    // ── Flex item properties ──────────────────────────────────────────────────
    if style.flex_grow != 0.0 {
        parts.push(("flex-grow".into(), style.flex_grow.to_string()));
    }
    if style.flex_shrink != 1.0 {
        parts.push(("flex-shrink".into(), style.flex_shrink.to_string()));
    }

    // ── Positioned offsets ────────────────────────────────────────────────────
    if style.position != Position::Static {
        let top_s = serialize_length(&style.top);
        let right_s = serialize_length(&style.right);
        let bottom_s = serialize_length(&style.bottom);
        let left_s = serialize_length(&style.left);
        if !top_s.is_empty()    { parts.push(("top".into(),    top_s)); }
        if !right_s.is_empty()  { parts.push(("right".into(),  right_s)); }
        if !bottom_s.is_empty() { parts.push(("bottom".into(), bottom_s)); }
        if !left_s.is_empty()   { parts.push(("left".into(),   left_s)); }
    }

    // ── z-index ───────────────────────────────────────────────────────────────
    if style.z_index != 0 {
        parts.push(("z-index".into(), style.z_index.to_string()));
    }

    // ── Opacity ───────────────────────────────────────────────────────────────
    if style.opacity < 1.0 {
        parts.push(("opacity".into(), format!("{:.3}", style.opacity)));
    }

    // ── Assemble ──────────────────────────────────────────────────────────────
    parts.iter()
        .map(|(k, v)| format!("{}: {}", k, v))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Convenience wrapper: serialize a `ComputedStyle` without a tag context.
pub fn serialize_style(style: &ComputedStyle) -> String {
    serialize_style_to_css(style, "")
}

// ─── Selector serialization ───────────────────────────────────────────────────

fn serialize_selector(sel: &CssSelector) -> String {
    let mut out = String::new();
    for part in &sel.parts {
        match part {
            SelectorPart::Tag(t)       => out.push_str(t),
            SelectorPart::Id(id)       => { out.push('#'); out.push_str(id); }
            SelectorPart::Class(cls)   => { out.push('.'); out.push_str(cls); }
            SelectorPart::Universal    => out.push('*'),
            SelectorPart::PseudoClass(pc) => {
                out.push(':');
                out.push_str(pc);
            }
            SelectorPart::PseudoElement(pe) => {
                out.push_str("::");
                out.push_str(pe);
            }
            SelectorPart::Attribute { name, op, value } => {
                use crate::css::AttrOp;
                out.push('[');
                out.push_str(name);
                match op {
                    AttrOp::Exists     => {}
                    AttrOp::Eq         => { out.push('=');  out.push('"'); out.push_str(value); out.push('"'); }
                    AttrOp::Includes   => { out.push_str("~=\""); out.push_str(value); out.push('"'); }
                    AttrOp::StartsWith => { out.push_str("^=\""); out.push_str(value); out.push('"'); }
                    AttrOp::EndsWith   => { out.push_str("$=\""); out.push_str(value); out.push('"'); }
                    AttrOp::Contains   => { out.push_str("*=\""); out.push_str(value); out.push('"'); }
                    AttrOp::DashMatch  => { out.push_str("|=\""); out.push_str(value); out.push('"'); }
                }
                out.push(']');
            }
            SelectorPart::Combinator(c) => {
                match c {
                    Combinator::Descendant      => out.push(' '),
                    Combinator::Child           => out.push_str(" > "),
                    Combinator::AdjacentSibling => out.push_str(" + "),
                    Combinator::GeneralSibling  => out.push_str(" ~ "),
                }
            }
            SelectorPart::Not(inner) => {
                out.push_str(":not(");
                out.push_str(&serialize_selector(inner));
                out.push(')');
            }
            SelectorPart::Is(list) => {
                out.push_str(":is(");
                for (i, s) in list.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    out.push_str(&serialize_selector(s));
                }
                out.push(')');
            }
            SelectorPart::Where(list) => {
                out.push_str(":where(");
                for (i, s) in list.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    out.push_str(&serialize_selector(s));
                }
                out.push(')');
            }
            SelectorPart::Has(inner) => {
                out.push_str(":has(");
                out.push_str(&serialize_selector(inner));
                out.push(')');
            }
        }
    }
    out
}

fn serialize_rule(rule: &CssRule) -> String {
    // Prefer the verbatim original selector for a faithful roundtrip (preserves
    // vendor pseudo-elements, whitespace, unknown syntax, etc.).  Fall back to
    // reconstructing from parsed parts only for programmatically-added rules.
    let sel_text = if !rule.original_selector.is_empty() {
        rule.original_selector.clone()
    } else {
        rule.selectors.iter()
            .map(serialize_selector)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    };

    if sel_text.is_empty() {
        return String::new();
    }

    let mut decls = String::new();
    // Use a sorted iteration for deterministic output.
    let mut props: Vec<(&String, &String)> = rule.declarations.iter().collect();
    props.sort_by_key(|(k, _)| k.as_str());
    for (prop, val) in props {
        decls.push_str(&format!("  {}: {};\n", prop, val));
    }
    let mut imp_props: Vec<(&String, &String)> = rule.important_declarations.iter().collect();
    imp_props.sort_by_key(|(k, _)| k.as_str());
    for (prop, val) in imp_props {
        decls.push_str(&format!("  {}: {} !important;\n", prop, val));
    }

    format!("{} {{\n{}}}\n", sel_text, decls)
}

fn serialize_stylesheet(doc: &Document) -> String {
    // Skip UA rules — only emit author rules (appended after UA rules in combined stylesheet).
    let ua_count = ua_stylesheet().rules.len();
    let author_rules: Vec<_> = doc.stylesheet.rules.iter().skip(ua_count).collect();

    if author_rules.is_empty() {
        return String::new();
    }

    let mut out = String::from("<style>\n");
    for rule in author_rules {
        let s = serialize_rule(rule);
        if !s.is_empty() {
            out.push_str(&s);
        }
    }
    out.push_str("</style>\n");
    out
}

// ─── Box serialization ────────────────────────────────────────────────────────

/// Serialize a single `HtmlBox` (and all its descendants) into `out`.
pub fn serialize_box(node: &HtmlBox, out: &mut String) {
    // Text nodes
    if node.tag == "#text" {
        out.push_str(&escape_html(&node.text));
        return;
    }

    let tag = if node.tag.is_empty() { "div" } else { node.tag.as_str() };

    // Open tag
    out.push('<');
    out.push_str(tag);

    // Attributes from the map — skip the field-managed ones that get their
    // own dedicated handling below.
    const FIELD_ATTRS: &[&str] = &["id", "class", "style", "src", "href"];
    let mut attr_pairs: Vec<(&String, &String)> = node.attributes.iter()
        .filter(|(k, _)| !FIELD_ATTRS.contains(&k.as_str()))
        .collect();
    attr_pairs.sort_by_key(|(k, _)| k.as_str()); // deterministic output
    for (k, v) in attr_pairs {
        out.push(' ');
        out.push_str(k);
        if !v.is_empty() {
            out.push_str("=\"");
            out.push_str(&escape_html(v));
            out.push('"');
        }
    }

    // Field-managed attributes (may have been modified after parse).
    if let Some(id) = node.attributes.get("id") {
        if !id.is_empty() {
            out.push_str(" id=\"");
            out.push_str(&escape_html(id));
            out.push('"');
        }
    }
    if let Some(cls) = node.attributes.get("class") {
        if !cls.is_empty() {
            out.push_str(" class=\"");
            out.push_str(&escape_html(cls));
            out.push('"');
        }
    }
    if let Some(src) = node.attributes.get("src") {
        if !src.is_empty() {
            out.push_str(" src=\"");
            out.push_str(&escape_html(src));
            out.push('"');
        }
    }
    if let Some(href) = node.attributes.get("href") {
        if !href.is_empty() {
            out.push_str(" href=\"");
            out.push_str(&escape_html(href));
            out.push('"');
        }
    }
    // Inline style (from `style` attribute, if present).
    if let Some(inline_css) = node.attributes.get("style") {
        if !inline_css.is_empty() {
            out.push_str(" style=\"");
            out.push_str(&escape_html(inline_css));
            out.push('"');
        }
    }

    // Void elements: self-close, no children.
    if node.is_void() {
        out.push('>');
        return;
    }

    out.push('>');

    // Own text (block-level text content stored directly on the node).
    if !node.text.is_empty() {
        out.push_str(&escape_html(&node.text));
    }

    // Inline runs — emit styled text segments.
    // Our InlineRun has text_offset, length, style but no atomicBox pointer,
    // so we just emit the text slice with light markup for common properties.
    for run in &node.inline_runs {
        let end = (run.text_offset + run.length).min(node.text.len());
        if run.text_offset > node.text.len() {
            continue;
        }
        let segment = &node.text[run.text_offset..end];
        if segment.is_empty() {
            continue;
        }

        let has_bold      = run.style.font_weight.is_bold();
        let has_italic    = run.style.font_style == FontStyle::Italic;
        let has_underline = run.style.text_decoration.underline;
        let has_strike    = run.style.text_decoration.strikethrough;
        let has_link      = node.attributes.get("href")
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        if has_link {
            if let Some(href) = node.attributes.get("href") {
                out.push_str("<a href=\"");
                out.push_str(&escape_html(href));
                out.push_str("\">");
            }
        }
        if has_bold      { out.push_str("<b>"); }
        if has_italic    { out.push_str("<i>"); }
        if has_underline && !has_link { out.push_str("<u>"); }
        if has_strike    { out.push_str("<s>"); }

        out.push_str(&escape_html(segment));

        if has_strike    { out.push_str("</s>"); }
        if has_underline && !has_link { out.push_str("</u>"); }
        if has_italic    { out.push_str("</i>"); }
        if has_bold      { out.push_str("</b>"); }
        if has_link      { out.push_str("</a>"); }
    }

    // Children (depth-first).
    for child in &node.children {
        serialize_box(child, out);
    }

    // Close tag.
    out.push_str("</");
    out.push_str(tag);
    out.push('>');
}

// ─── Document serialization ───────────────────────────────────────────────────

/// Serialize the full document tree to an HTML string.
///
/// * If the document has stylesheet rules, the output is wrapped in a full
///   `<html><head><style>…</style></head><body>…</body></html>` document.
/// * Otherwise only the children of the root "html" box are serialized.
pub fn serialize_html(doc: &Document) -> String {
    let style_block = serialize_stylesheet(doc);

    let mut html = String::new();

    // Serialize the root box.  When the root is the synthetic "html" wrapper
    // (tag == "html" with no attributes of its own) we emit only its children,
    // matching the C++ behaviour of skipping the root tag.
    let root = &doc.root;
    let root_is_synthetic = root.tag == "html"
        && root.attributes.is_empty()
        && root.text.is_empty();

    if !style_block.is_empty() {
        html.push_str("<html>\n<head>\n");
        html.push_str(&style_block);
        html.push_str("</head>\n<body>");

        // When emitting an html/head/body wrapper, skip the body tag in the DOM
        // tree to avoid double-nesting (<body><body>…</body></body>).
        if root_is_synthetic {
            for child in &root.children {
                if child.tag == "body" {
                    for grandchild in &child.children {
                        serialize_box(grandchild, &mut html);
                    }
                } else {
                    serialize_box(child, &mut html);
                }
            }
        } else if root.tag == "body" {
            for child in &root.children {
                serialize_box(child, &mut html);
            }
        } else {
            serialize_box(root, &mut html);
        }

        html.push_str("</body>\n</html>");
    } else if root_is_synthetic {
        for child in &root.children {
            serialize_box(child, &mut html);
        }
    } else {
        serialize_box(root, &mut html);
    }

    html
}
