//! Tests for CSS value AST: min(), max(), clamp(), calc() with proper resolution.

use crate::types::CssLength;
use crate::css::parse_length;

fn resolve(val: &CssLength, containing: f32, vw: f32) -> f32 {
    val.resolve_vp(16.0, containing, 16.0, vw, 600.0)
}

// ── Basic parsing ──────────────────────────────────────────────────────────

#[test]
fn parse_px() {
    assert_eq!(parse_length("100px"), CssLength::Px(100.0));
}

#[test]
fn parse_percent() {
    assert_eq!(parse_length("50%"), CssLength::Percent(50.0));
}

#[test]
fn parse_em() {
    assert_eq!(parse_length("2em"), CssLength::Em(2.0));
}

#[test]
fn parse_rem() {
    assert_eq!(parse_length("1.5rem"), CssLength::Rem(1.5));
}

#[test]
fn parse_vw() {
    assert_eq!(parse_length("100vw"), CssLength::Vw(100.0));
}

#[test]
fn parse_auto() {
    assert_eq!(parse_length("auto"), CssLength::Auto);
}

// ── min() ──────────────────────────────────────────────────────────────────

#[test]
fn parse_min_two_args() {
    let val = parse_length("min(300px, 50%)");
    match &val {
        CssLength::Min(args) => assert_eq!(args.len(), 2),
        other => panic!("expected Min, got {:?}", other),
    }
}

#[test]
fn min_resolves_to_smaller() {
    let val = parse_length("min(300px, 50%)");
    // 50% of 800px = 400px, min(300, 400) = 300
    assert_eq!(resolve(&val, 800.0, 1024.0), 300.0);
    // 50% of 400px = 200px, min(300, 200) = 200
    assert_eq!(resolve(&val, 400.0, 1024.0), 200.0);
}

#[test]
fn min_with_vw() {
    let val = parse_length("min(100vw, 1200px)");
    // viewport 800: min(800, 1200) = 800
    assert_eq!(resolve(&val, 0.0, 800.0), 800.0);
    // viewport 1400: min(1400, 1200) = 1200
    assert_eq!(resolve(&val, 0.0, 1400.0), 1200.0);
}

// ── max() ──────────────────────────────────────────────────────────────────

#[test]
fn parse_max_two_args() {
    let val = parse_length("max(200px, 30%)");
    match &val {
        CssLength::Max(args) => assert_eq!(args.len(), 2),
        other => panic!("expected Max, got {:?}", other),
    }
}

#[test]
fn max_resolves_to_larger() {
    let val = parse_length("max(200px, 30%)");
    // 30% of 800px = 240px, max(200, 240) = 240
    assert!((resolve(&val, 800.0, 1024.0) - 240.0).abs() < 0.01);
    // 30% of 500px = 150px, max(200, 150) = 200
    assert!((resolve(&val, 500.0, 1024.0) - 200.0).abs() < 0.01);
}

// ── clamp() ────────────────────────────────────────────────────────────────

#[test]
fn parse_clamp_three_args() {
    let val = parse_length("clamp(200px, 50%, 600px)");
    match &val {
        CssLength::Clamp(_) => {} // ok
        other => panic!("expected Clamp, got {:?}", other),
    }
}

#[test]
fn clamp_resolves_correctly() {
    let val = parse_length("clamp(200px, 50%, 600px)");
    // 50% of 300 = 150 → clamped to min 200
    assert_eq!(resolve(&val, 300.0, 1024.0), 200.0);
    // 50% of 800 = 400 → within range
    assert_eq!(resolve(&val, 800.0, 1024.0), 400.0);
    // 50% of 1400 = 700 → clamped to max 600
    assert_eq!(resolve(&val, 1400.0, 1024.0), 600.0);
}

#[test]
fn clamp_with_mixed_units() {
    // Common responsive pattern: clamp(1rem, 2.5vw, 2rem)
    let val = parse_length("clamp(1rem, 2.5vw, 2rem)");
    // 1rem=16px, 2rem=32px, 2.5vw at 800px = 20px → within range
    assert_eq!(resolve(&val, 0.0, 800.0), 20.0);
    // 2.5vw at 500px = 12.5px → clamped to 1rem = 16px
    assert_eq!(resolve(&val, 0.0, 500.0), 16.0);
    // 2.5vw at 1600px = 40px → clamped to 2rem = 32px
    assert_eq!(resolve(&val, 0.0, 1600.0), 32.0);
}

// ── Nested ─────────────────────────────────────────────────────────────────

#[test]
fn nested_min_in_max() {
    // max(300px, min(50%, 600px))
    let val = parse_length("max(300px, min(50%, 600px))");
    // 50% of 800 = 400, min(400,600) = 400, max(300,400) = 400
    assert_eq!(resolve(&val, 800.0, 1024.0), 400.0);
    // 50% of 400 = 200, min(200,600) = 200, max(300,200) = 300
    assert_eq!(resolve(&val, 400.0, 1024.0), 300.0);
}

#[test]
fn clamp_with_calc() {
    // clamp(200px, calc(100% - 40px), 800px)
    let val = parse_length("clamp(200px, calc(100% - 40px), 800px)");
    // 100% of 500 - 40 = 460 → within range
    let r = resolve(&val, 500.0, 1024.0);
    assert!((r - 460.0).abs() < 1.0, "expected ~460, got {}", r);
    // 100% of 100 - 40 = 60 → clamped to 200
    assert_eq!(resolve(&val, 100.0, 1024.0), 200.0);
}

// ── Layout integration ─────────────────────────────────────────────────────

#[test]
fn min_width_in_layout() {
    let mut frame = crate::frame::EngineFrame::new(
        crate::html::parse_html(r#"<div id="box" style="width: min(300px, 80%)">content</div>"#),
        800.0, 600.0,
    );
    frame.update_frame();

    let id = frame.doc.get_element_by_id("box").unwrap();
    let w = frame.doc.offset_width(id);
    // 80% of ~800 = 640, min(300, 640) = 300
    assert!((w - 300.0).abs() < 5.0, "min(300px, 80%) at 800px viewport should be ~300px, got {}", w);
}

#[test]
fn clamp_width_in_layout() {
    let mut frame = crate::frame::EngineFrame::new(
        crate::html::parse_html(r#"<div id="box" style="width: clamp(200px, 50%, 600px)">content</div>"#),
        800.0, 600.0,
    );
    frame.update_frame();

    let id = frame.doc.get_element_by_id("box").unwrap();
    let w = frame.doc.offset_width(id);
    // 50% of containing block (~784px after body margins), clamp(200, ~392, 600) → ~392
    assert!(w > 200.0 && w < 600.0, "clamp(200,50%,600) should be between bounds, got {}", w);
}

// ── line-height is not a plain length ────────────────────────────────────────

/// **A unitless `line-height` is a MULTIPLE of the font size, not pixels**
/// (CSS 2.1 §10.8.1). The value pre-parser had `line-height` in its generic
/// length group, so `line-height: 1.375` pre-parsed to 1.375 **pixels** while
/// the string path read it as `1.375em`. The two cascade paths therefore
/// disagreed: fr.wikipedia rendered normally and then, on the first hover,
/// every heading lost its leading and `#firstHeading` collapsed to one pixel
/// tall — its text vanished.
#[test]
fn line_height_number_is_a_multiple_not_pixels() {
    for (input, expect) in [
        ("1.375", crate::types::CssLength::Em(1.375)),
        ("2",     crate::types::CssLength::Em(2.0)),
        ("normal", crate::types::CssLength::Em(1.2)),
    ] {
        // The string path.
        let mut s = crate::types::ComputedStyle::default();
        crate::css::apply_property(&mut s, "line-height", input);
        assert_eq!(s.line_height, expect, "string path for {input:?}");

        // The pre-parsed path, which is what the cascade normally takes.
        let id = crate::css::properties::PropertyId::LineHeight;
        let pre = crate::css::rule::pre_parse_value(id, input);
        let mut p = crate::types::ComputedStyle::default();
        crate::css::apply_css_value(&mut p, id, &pre);
        assert_eq!(p.line_height, expect, "pre-parsed path for {input:?} (pre-parsed as {pre:?})");
    }
}

/// A `line-height` written with a unit keeps that unit.
#[test]
fn line_height_with_a_unit_stays_a_length() {
    for input in ["20px", "1.5em", "150%", "2rem"] {
        let mut s = crate::types::ComputedStyle::default();
        crate::css::apply_property(&mut s, "line-height", input);
        let id = crate::css::properties::PropertyId::LineHeight;
        let pre = crate::css::rule::pre_parse_value(id, input);
        let mut p = crate::types::ComputedStyle::default();
        crate::css::apply_css_value(&mut p, id, &pre);
        assert_eq!(s.line_height, p.line_height,
            "the two value paths disagree about {input:?}");
    }
}

// ── background-image url() forms ─────────────────────────────────────────────

/// **A protocol-relative, unquoted `url()` is a URL.** fr.wikipedia's main
/// banner is `background-image:url(//upload.wikimedia.org/...svg)`; the rule
/// applies (its `display:flex` reaches the box) but the image never painted.
#[test]
fn background_image_url_forms_all_parse() {
    for input in [
        "url(//upload.wikimedia.org/wikipedia/commons/a/aa/Wikipedia-logo-v2-o50.svg)",
        "url('//upload.wikimedia.org/wikipedia/commons/a/aa/Wikipedia-logo-v2-o50.svg')",
        "url(\"//upload.wikimedia.org/wikipedia/commons/a/aa/Wikipedia-logo-v2-o50.svg\")",
    ] {
        let mut s = crate::types::ComputedStyle::default();
        crate::css::apply_property(&mut s, "background-image", input);
        assert_eq!(s.background_image_url,
            "//upload.wikimedia.org/wikipedia/commons/a/aa/Wikipedia-logo-v2-o50.svg",
            "background-image did not parse: {input}");
    }
}

/// And it must resolve against the document origin, keeping the scheme.
#[test]
fn protocol_relative_url_takes_the_base_scheme() {
    let got = crate::html::images::resolve_url(
        "//upload.wikimedia.org/a.svg", "https://fr.wikipedia.org/wiki/Foo");
    assert_eq!(got, "https://upload.wikimedia.org/a.svg");
}

// ── Colour keywords and accent-color ────────────────────────────────────────

/// **Colour keywords are ASCII case-insensitive.** Legacy presentational HTML
/// still ships `bgcolor="White"` and `color="Red"`; matching the keyword table
/// byte-exactly dropped the declaration and left the element at its default.
#[test]
fn colour_keywords_are_case_insensitive() {
    for (input, expect) in [("White", (255u8, 255u8, 255u8)), ("RED", (255, 0, 0)),
                            ("Blue", (0, 0, 255)), ("white", (255, 255, 255))] {
        let mut s = crate::types::ComputedStyle::default();
        crate::css::apply_property(&mut s, "background-color", input);
        let c = s.background_color;
        assert_eq!((c.r, c.g, c.b), expect, "background-color: {input}");
    }
}

/// **`accent-color` must not touch `background-color`.** With no accent field
/// of its own it was assigned straight into `background_color`, so it painted a
/// solid block and fought a real `background-color` in the same rule. Until the
/// accent is actually plumbed through to control painting, doing nothing is the
/// correct behaviour — doing damage is not.
#[test]
fn accent_color_does_not_overwrite_the_background() {
    let mut s = crate::types::ComputedStyle::default();
    crate::css::apply_property(&mut s, "background-color", "white");
    crate::css::apply_property(&mut s, "accent-color", "red");
    let c = s.background_color;
    assert_eq!((c.r, c.g, c.b), (255, 255, 255),
        "accent-color clobbered background-color: got {},{},{}", c.r, c.g, c.b);
}

/// `font-size: xxx-large` is a real absolute-size keyword; falling through to
/// the length parser made it `auto`, which resolves to **0** and the text
/// disappears.
#[test]
fn font_size_xxx_large_is_not_zero() {
    let mut s = crate::types::ComputedStyle::default();
    crate::css::apply_property(&mut s, "font-size", "xxx-large");
    let px = s.font_size.resolve(16.0, 0.0, 16.0);
    assert!(px > 30.0, "xxx-large should be larger than xx-large, got {px}");
}
