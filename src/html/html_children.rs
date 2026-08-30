//! The document-level children parser.

#![allow(unused_imports)]
use super::*;
use crate::types::*;
use crate::css::*;

// ─── html-level children parser ──────────────────────────────────────────────

pub(crate) fn parse_html_children(
    parser: &mut HtmlParser,
    html_box: &mut WebCore,
    body_box: &mut WebCore,
    body_children: &mut Vec<WebCore>,
    ol_counter: &mut i32,
) {
    loop {
        match parser.tokens.get(parser.pos).cloned() {
            None => break,
            Some(Token::CloseTag { tag }) if tag == "html" => {
                parser.pos += 1;
                break;
            }
            Some(Token::Comment(data)) => {
                parser.pos += 1;
                // A comment BEFORE `<html>` is a child of the Document, not of
                // the body — and this tree has no document node to hold it, so
                // it is dropped rather than moved somewhere it does not belong.
                if parser.head_closed {
                    let mut node = parser.new_box("#comment");
                    node.text = data;
                    apply_property(&mut node.style, "display", "none");
                    body_children.push(node);
                }
            }
            Some(Token::CloseTag { .. }) | Some(Token::Doctype(_)) => {
                parser.pos += 1;
            }
            Some(Token::Text(t)) => {
                parser.pos += 1;
                let collapsed = collapse_whitespace(&t);
                if !collapsed.trim().is_empty() {
                    // Non-blank text is content, so it closes the head too.
                    // Blank text does NOT — whitespace is ignored in "before
                    // head", which is why this sits inside the guard.
                    parser.head_closed = true;
                    let mut node = parser.new_box("#text");
                    node.text = collapsed;
                    let had_pending = !parser.pending_format.is_empty();
                    let from = body_children.len();
                    body_children.push(node);
                    parser.reconstruct_into(body_children, from, had_pending);
                }
            }
            Some(Token::OpenTag { tag, attrs, self_closing }) => {
                parser.pos += 1;
                match tag.as_str() {
                    "head" => {
                        if !parser.head_closed {
                            if !self_closing {
                                parse_head_content(parser);
                            }
                            parser.head_closed = true;
                        }
                        // else: parse error, token ignored — see the sibling
                        // arm in `parse_html_document`.
                    }
                    "body" => {
                        parser.head_closed = true;
                        body_box.attributes = attrs;
                        apply_property(&mut body_box.style, "display", "block");
                        apply_presentational_attrs(body_box);
                        if !self_closing {
                            parser.parse_children_into("body", body_children, ol_counter);
                        }
                    }
                    // Head content met before any body content belongs to the
                    // IMPLIED head (§13.2.6.4.3 "before head" → "in head"),
                    // even though the document never wrote `<head>`. A page
                    // that opens with a bare `<style>` — which most of the
                    // example pages do — was putting it in the body, so
                    // `document.head` was empty and the element sat in the
                    // flow. Chrome puts it in the head; so does this now.
                    _ if !parser.head_closed && is_head_content_tag(&tag) => {
                        handle_head_tag(parser, &tag, attrs, self_closing);
                    }
                    _ => {
                        // Anything else is body content, and body content ends
                        // the implied head.
                        parser.head_closed = true;
                        let had_pending = !parser.pending_format.is_empty();
                        let from = body_children.len();
                        parser.handle_tag(tag, attrs, self_closing, body_children, ol_counter);
                        parser.reconstruct_into(body_children, from, had_pending);
                    }
                }
                // Suppress unused warning for html_box (it's passed by mut for possible future use)
                let _ = &html_box;
            }
        }
    }
}
