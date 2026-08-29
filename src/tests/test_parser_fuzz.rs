//! The parser must not fault on ANY input.
//!
//! A browser meets truncated and malformed markup constantly — a page cut off
//! mid-download, a hand-written fragment, a template that lost its close tag —
//! and the spec's tokenizer has a defined state for every one of them. There is
//! no input for which "panic" is the right answer.
//!
//! This is a shape test, not a conformance test: it asserts only that parsing
//! RETURNS. It caught nothing on its own the day it was written, because the
//! bug that motivated it (`<!-->` building a backwards byte range, which took
//! the tokenizer down) was already fixed — but that bug reached a 200-case
//! conformance suite without being caught, because a panic ABORTS the run and
//! everything behind it silently stops being checked.

use crate::parse_html;

/// Every prefix of `src`, so a document truncated at any byte is exercised.
fn truncations(src: &str) -> impl Iterator<Item = &str> {
    (0..=src.len()).filter(move |i| src.is_char_boundary(*i)).map(move |i| &src[..i])
}

#[test]
fn parsing_a_truncated_document_never_panics() {
    // Deliberately dense in the constructs that carry state across bytes:
    // comments, raw text, entities, attributes, nested formatting, tables.
    let src = concat!(
        "<!DOCTYPE html><html><head><title>T&amp;t</title>",
        "<style>a > b { color: red }</style></head><body>",
        "<!-- c -- d --><div class=\"x\" data-a='1'>text&#38;more&notin;</div>",
        "<table><caption>c</caption><tr><td>a<td>b</table>",
        "<p><b>1<i>2</b>3</i></p><select><option selected>o</select>",
        "<textarea>&lt;p&gt;</textarea><script>var s = \"</p>\";</script>",
        "<template><td>t</td></template><frameset><frame src=f></frameset>",
        "</body></html>",
    );
    for prefix in truncations(src) {
        let _ = parse_html(prefix);
    }
}

#[test]
fn parsing_malformed_markup_never_panics() {
    // Each of these is a token the tokenizer can fall off the end of, or a
    // shape that has previously produced a bad index.
    const INPUTS: &[&str] = &[
        "", "<", "<>", "</", "</>", "<!", "<!-", "<!--", "<!-->", "<!--->",
        "<!---->", "<!doctype", "<?", "<?x", "<//", "< ", "<3", "&", "&#",
        "&#x", "&#x;", "&#;", "&#xZZ;", "&notarealentity;", "&#999999999;",
        "<div", "<div ", "<div a", "<div a=", "<div a=\"", "<div a=\"x",
        "<div a='", "<div/", "<div//", "<div / ", "<p<p>", "<div a=b=c>",
        "<div \"q\">", "<div ='v'>", "<script>", "<script>x", "<style>",
        "<title>", "<textarea>", "<noscript>", "<table><tr><td>",
        "<select><option>", "<b><i><u>", "<!--<div>-->", "</div></div></div>",
        "<div></", "<div></div", "\u{0}", "<div>\u{0}</div>", "<\u{0}div>",
        "\u{FFFD}", "<div a=\u{0}>", "<!--\u{0}-->",
    ];
    for input in INPUTS {
        let _ = parse_html(input);
    }
}

#[test]
fn deeply_nested_markup_does_not_overflow() {
    // ⚠ MEASURED LIMIT: parsing costs roughly **64 KB of STACK per nesting
    // level** in a debug build, so ~120 elements deep fits an 8 MB stack.
    // It was 100-128 KB (and ~80 deep) before `CssLength` was shrunk from 32
    // to 16 bytes, which took `ComputedStyle` from 3352 to 2328.
    //
    // The cost is the CASCADE, which `parse_html` runs: `apply_cascade_inner`
    // recurses per element and holds several `ComputedStyle` values (2328
    // bytes each) plus two `WebCore` pseudo-element boxes live across the
    // recursive call. The parser's own tree walks were rewritten to work in
    // place for the same reason.
    //
    // This test runs on a thread with a browser-sized stack and asserts a
    // depth real pages reach. It is a BOUNDARY, not a target: the fix is to
    // shrink the cascade's frame, and when that lands this number should go up
    // and the note above should come down.
    let handle = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let deep = "<div>".repeat(100);
            let _ = parse_html(&deep);
            let closed = format!("{}{}", "<div>".repeat(100), "</div>".repeat(100));
            let _ = parse_html(&closed);
            let formatting = "<b>".repeat(100);
            let _ = parse_html(&formatting);
            // Auto-closing elements must not nest at all, however many arrive.
            let flat = "<p>x".repeat(2000);
            let _ = parse_html(&flat);
            let siblings = "<div>x</div>".repeat(5000);
            let _ = parse_html(&siblings);
        })
        .expect("spawn");
    handle.join().expect("parsing 50-deep markup must not fault");
}
