// Pixel-level render tests for blend modes, gradients, and layout.
// ── Button background covers right padding ───────────────────────────────────
// Pixel test: the background color must appear in the right-padding zone.
#[test]
fn render_button_bg_covers_right_padding() {
    // Button with red background, 20px left/right padding.
    // After layout, the right 20px zone should be red.
    let pm = render_html(r#"
        <style>
        * { margin: 0; padding: 0; }
        body { background: white; }
        .row { display: flex; }
        .btn { padding: 0 20px; background: red; font-size: 10pt; }
        </style>
        <div class="row">
          <div class="btn">Hi</div>
        </div>
    "#, 200, 40);
    // The button starts at x=0. Find a non-white pixel to locate button right edge.
    // Scan row y=5 for the last red pixel.
    let y = 5u32;
    let mut last_red = None;
    for x in 0..200 {
        let (r, g, b, _a) = pixel(&pm, x, y);
        if r > 200 && g < 50 && b < 50 {
            last_red = Some(x);
        }
    }
    let last_red = last_red.expect("No red pixel found — button background not rendered");
    // Inside the right padding, a pixel 5 from the last_red edge should also be red
    // i.e., the rightmost-20px zone is all red. Test that at least 15 consecutive red
    // pixels exist before last_red.
    let mut run = 0u32;
    for x in (0..=last_red).rev() {
        let (r, g, b, _a) = pixel(&pm, x, y);
        if r > 200 && g < 50 && b < 50 { run += 1; } else { break; }
    }
    assert!(run >= 15,
        "Right padding area should be at least 15px red; got {} red pixels ending at x={}",
        run, last_red);
}

// ── Float:right text renders at correct position ─────────────────────────────
// Pixel test: text in a float:right span must appear on the right half.
#[test]
fn render_float_right_text_visible() {
    // White background, float:right span colored red.
    let pm = render_html(r#"
        <style>
        * { margin: 0; padding: 0; }
        body { background: white; }
        .item { width: 200px; background: #eee; font-size: 10pt; }
        .stat { float: right; color: red; }
        </style>
        <div class="item">Left <span class="stat">Rt</span></div>
    "#, 300, 40);
    // Expect some colored pixels on the right half (x > 100) of the div
    let mut found_right = false;
    for x in 100..200 {
        for y in 0..30u32 {
            let (r, _g, _b, a) = pixel(&pm, x, y);
            // Red text should have r component significantly higher
            if a > 10 && r > 150 {
                found_right = true;
                break;
            }
        }
        if found_right { break; }
    }
    assert!(found_right, "float:right text (red) should appear in right half of container");
}

// ── Button background in graph_demo exact setup ──────────────────────────────
// Pixel test: button background (with border-radius) covers right padding
// in a border-box sidebar+content layout exactly matching graph_demo CSS.
#[test]
fn render_graph_demo_button_bg_right_padding() {
    // Matches graph_demo: * { box-sizing: border-box }, sidebar 170px,
    // content flex:1, button padding 5px 12px, border-radius 6px.
    let pm = render_html(r#"
        <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { background: #0d1117; }
        .main { display: flex; }
        .sidebar { width: 170px; min-width: 170px; background: #161b22;
                   border-right: 1px solid #30363d; padding: 14px; }
        .content { flex: 1; }
        .btn-row { display: flex; gap: 8px; padding: 0 16px 12px 16px; }
        .btn { padding: 5px 12px; border-radius: 6px; font-size: 8pt;
               font-weight: 600; border: none; background: #1f6feb; color: #fff; }
        </style>
        <div class="main">
          <div class="sidebar">S</div>
          <div class="content">
            <div class="btn-row">
              <div class="btn" id="b1">All Bar</div>
              <div class="btn" id="b2">All Line</div>
            </div>
          </div>
        </div>
    "#, 800, 60);
    // The first button starts at x = 170 (sidebar) + 16 (btn-row pad-left) = 186.
    // It should be blue (#1f6feb → r=31 g=111 b=235) across its full width.
    // Scan y=15 (inside the button, away from top/bottom padding) for blue pixels.
    let y = 15u32;
    let mut first_blue = None;
    let mut last_blue = None;
    for x in 186..600 {
        let (r, _g, b, _a) = pixel(&pm, x, y);
        // button blue: high blue, low red
        if b > 150 && r < 100 {
            if first_blue.is_none() { first_blue = Some(x); }
            last_blue = Some(x);
        } else if last_blue.is_some() {
            break; // past the first button
        }
    }
    let first = first_blue.expect("No blue pixel found — button background not rendered");
    let last  = last_blue.unwrap();
    let btn_w = (last - first + 1) as f32;
    // Button should be at least content(~35px) + 12 + 12 = ~59px wide
    assert!(btn_w >= 40.0,
        "Button blue area should be >= 40px wide (includes both paddings), got {btn_w}px [{first}..{last}]");
    // The rightmost blue pixel should be at least 10px past the text start
    // (i.e., right padding is present). Text starts ~12px from first blue.
    assert!((last - first) >= 30,
        "Button should span at least 30px of blue (text + both paddings), got {} [{first}..{last}]",
        last - first);
}

// ── Bold text background covers right padding (border-radius demo) ────────────
// Regression: measure_text_width_weighted underestimated bold text width when
// no font system was present, making content_w too small → right padding gap.
// Fix: apply 1.15x multiplier for bold/semi-bold in the approximation path.
#[test]
fn render_bold_text_bg_covers_right_padding() {
    // Mirror the "Border Radius" section from demo.html:
    // flex row of bold-text divs with blue bg + 20px padding + border-radius.
    let pm = render_html(r#"
        <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { background: white; }
        .row { display: flex; gap: 16px; padding: 10px; }
        .card { background-color: #3498db; color: white;
                padding: 20px; border-radius: 8px; font-weight: bold; font-size: 10pt; }
        </style>
        <div class="row">
          <div class="card">8px radius</div>
        </div>
    "#, 300, 80);
    // Scan row y=30 (inside the card, away from top/bottom padding rounding).
    // Find the first and last blue pixels — blue: high B, low R.
    let y = 30u32;
    let mut first_blue: Option<u32> = None;
    let mut last_blue:  Option<u32> = None;
    for x in 0..300 {
        let (r, _g, b, _a) = pixel(&pm, x, y);
        if b > 150 && r < 100 {
            if first_blue.is_none() { first_blue = Some(x); }
            last_blue = Some(x);
        }
    }
    let first = first_blue.expect("No blue pixel found — bold card background not rendered");
    let last  = last_blue.unwrap();
    let span  = last - first + 1;
    // Card has 20px left + text ("8px radius" ~9 chars * ~7px ≈ 63px) + 20px right.
    // Total ≥ 100px. Require ≥ 80px to have headroom for font approximation variance
    // but still catch the bug (pre-fix the background was ~63px with no right padding).
    assert!(span >= 80,
        "Bold card background should span ≥ 80px (text + both 20px paddings), got {span}px [{first}..{last}]");
    // The right padding must be present: at least 10px of blue after the text.
    // We don't know the exact text width, but the blue run must extend well past
    // what the text alone would cover (without right-pad the span would be ~63px).
    assert!(span >= 90,
        "Bold card background right padding appears missing: span={span}px, expected ≥ 90px [{first}..{last}]");
}

// Pixel-level render tests for blend modes and gradients.

use tiny_skia::Pixmap;
use crate::renderer::Renderer;
use super::harness::parse_and_layout;

fn render_html(html: &str, w: u32, h: u32) -> Pixmap {
    let mut renderer = Renderer::new();
    // Use renderer.load_html so layout and rendering share the same font system,
    // ensuring background rects are sized to actual glyph widths (not approximations).
    let mut doc = renderer.load_html(html, w as f32);
    let mut pixmap = Pixmap::new(w, h).unwrap();
    renderer.render(&mut doc, &mut pixmap, 1.0);
    pixmap
}

fn pixel(pm: &Pixmap, x: u32, y: u32) -> (u8, u8, u8, u8) {
    let idx = (y * pm.width() + x) as usize * 4;
    let d = pm.data();
    // tiny-skia stores premultiplied RGBA
    let a = d[idx + 3];
    if a == 0 { return (0, 0, 0, 0); }
    // un-premultiply
    let r = ((d[idx]     as u32 * 255) / a as u32) as u8;
    let g = ((d[idx + 1] as u32 * 255) / a as u32) as u8;
    let b = ((d[idx + 2] as u32 * 255) / a as u32) as u8;
    (r, g, b, a)
}

// ── Flex nav: li items inside a flex ul must not overlap ──────────────────────

#[test]
fn layout_flex_nav_li_items_no_overlap() {
    // A flex <ul> with <li> items: each li must start after the previous one ends.
    // Historically, compute_intrinsic_width treated list-item children as block children
    // (taking max width), which caused the outer flex to assign too-small a width to the
    // ul, which then squished all li items to near-zero causing visual overlap.
    use super::harness::find_box;
    let doc = parse_and_layout(r#"
        <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        .nav { display: flex; width: 600px; }
        .links { display: flex; gap: 20px; list-style: none; }
        </style>
        <nav class="nav">
          <ul class="links">
            <li>World</li>
            <li>Politics</li>
            <li>Science</li>
          </ul>
        </nav>
    "#, 600.0);

    // Collect li border_rect x positions
    let mut li_boxes: Vec<f32> = Vec::new();
    fn collect_li(node: &crate::types::HtmlBox, out: &mut Vec<f32>) {
        if node.tag == "li" { out.push(node.border_rect.x); }
        for ch in &node.children { collect_li(ch, out); }
    }
    collect_li(&doc.root, &mut li_boxes);

    assert_eq!(li_boxes.len(), 3, "expected 3 li boxes, got {}", li_boxes.len());
    // Each li must start strictly after the previous one (no overlap)
    assert!(li_boxes[1] > li_boxes[0] + 5.0,
        "Politics li should start after World li; World.x={} Politics.x={}", li_boxes[0], li_boxes[1]);
    assert!(li_boxes[2] > li_boxes[1] + 5.0,
        "Science li should start after Politics li; Politics.x={} Science.x={}", li_boxes[1], li_boxes[2]);
}

// ── Absolute positioned child height from inset: 0 ───────────────────────────

#[test]
fn layout_abs_inset_zero_fills_parent_height() {
    // position:absolute; inset:0 must give the child the same height as the parent.
    use super::harness::find_box;
    let doc = parse_and_layout(r#"
        <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        .parent { width: 200px; height: 100px; position: relative; }
        .child  { position: absolute; inset: 0; }
        </style>
        <div class="parent"><div class="child"></div></div>
    "#, 200.0);
    let child = find_box(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "child").unwrap_or(false)
    }).expect("child not found");
    assert!((child.border_rect.w - 200.0).abs() < 1.0,
        "inset:0 child width should be 200, got {}", child.border_rect.w);
    assert!((child.border_rect.h - 100.0).abs() < 1.0,
        "inset:0 child height should be 100, got {}", child.border_rect.h);
}

// ── Blend mode: solid colors ──────────────────────────────────────────────────

#[test]
fn render_blend_multiply_solid_colors() {
    // Red stage (255,0,0) + blue overlay (inset:0) with multiply → near-black
    let pm = render_html(r#"
        <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        .stage { width: 100px; height: 100px; background: #ff0000; position: relative; }
        .overlay { position: absolute; inset: 0; background: #0000ff; mix-blend-mode: multiply; }
        </style>
        <div class="stage"><div class="overlay"></div></div>
    "#, 100, 100);
    let (r, g, b, _) = pixel(&pm, 50, 50);
    // multiply(red, blue) = (255*0/255, 0*0/255, 0*255/255) = (0,0,0) → black
    assert!(r < 20, "multiply red*blue should give near-black red channel, got {r}");
    assert!(b < 20, "multiply red*blue should give near-black blue channel, got {b}");
}

#[test]
fn render_blend_screen_solid_colors() {
    // Red stage (255,0,0) + blue overlay (inset:0) with screen → bright magenta
    let pm = render_html(r#"
        <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        .stage { width: 100px; height: 100px; background: #ff0000; position: relative; }
        .overlay { position: absolute; inset: 0; background: #0000ff; mix-blend-mode: screen; }
        </style>
        <div class="stage"><div class="overlay"></div></div>
    "#, 100, 100);
    let (r, g, b, _) = pixel(&pm, 50, 50);
    // screen(red, blue) = 1-(1-1)*(1-0)=1 for R; 1-(1-0)*(1-1)=1 for B → magenta (255,0,255)
    assert!(r > 200, "screen red*blue should give bright red channel, got {r}");
    assert!(b > 200, "screen red*blue should give bright blue channel, got {b}");
}

#[test]
fn render_blend_normal_vs_multiply_differ() {
    // Verify that multiply and normal produce different pixels
    let normal_pm = render_html(r#"
        <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        .stage { width: 100px; height: 100px; background: #ff6600; position: relative; }
        .overlay { position: absolute; inset: 0; background: #0066ff; mix-blend-mode: normal; }
        </style>
        <div class="stage"><div class="overlay"></div></div>
    "#, 100, 100);
    let multiply_pm = render_html(r#"
        <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        .stage { width: 100px; height: 100px; background: #ff6600; position: relative; }
        .overlay { position: absolute; inset: 0; background: #0066ff; mix-blend-mode: multiply; }
        </style>
        <div class="stage"><div class="overlay"></div></div>
    "#, 100, 100);
    let (nr, ng, nb, _) = pixel(&normal_pm, 50, 50);
    let (mr, mg, mb, _) = pixel(&multiply_pm, 50, 50);
    // normal shows the blue overlay; multiply: orange*blue = much darker
    assert_ne!((nr, nb), (mr, mb),
        "normal and multiply should produce different pixels; normal=({nr},{ng},{nb}) multiply=({mr},{mg},{mb})");
    let normal_luma = nr as u32 + ng as u32 + nb as u32;
    let multiply_luma = mr as u32 + mg as u32 + mb as u32;
    assert!(multiply_luma < normal_luma,
        "multiply should be darker than normal; normal_luma={normal_luma} multiply_luma={multiply_luma}");
}

// ── Blend mode: with radial gradient ─────────────────────────────────────────

#[test]
fn render_blend_multiply_gradient_overlay() {
    // Linear base + radial warm overlay (inset:0) with multiply → center darker than normal
    let normal_pm = render_html(r#"
        <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        .stage { width: 200px; height: 100px; background: linear-gradient(90deg, #1d4ed8, #be185d);
                 position: relative; }
        .overlay { position: absolute; inset: 0;
                   background: radial-gradient(circle at 50% 50%, #fbbf24 0%, #f97316 60%, transparent 100%);
                   mix-blend-mode: normal; }
        </style>
        <div class="stage"><div class="overlay"></div></div>
    "#, 200, 100);
    let multiply_pm = render_html(r#"
        <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        .stage { width: 200px; height: 100px; background: linear-gradient(90deg, #1d4ed8, #be185d);
                 position: relative; }
        .overlay { position: absolute; inset: 0;
                   background: radial-gradient(circle at 50% 50%, #fbbf24 0%, #f97316 60%, transparent 100%);
                   mix-blend-mode: multiply; }
        </style>
        <div class="stage"><div class="overlay"></div></div>
    "#, 200, 100);
    let (nr, ng, nb, _) = pixel(&normal_pm, 100, 50);
    let (mr, mg, mb, _) = pixel(&multiply_pm, 100, 50);
    assert!(
        mr != nr || mg != ng || mb != nb,
        "multiply and normal should differ at center; normal=({nr},{ng},{nb}) multiply=({mr},{mg},{mb})"
    );
    let normal_luma = nr as u32 + ng as u32 + nb as u32;
    let multiply_luma = mr as u32 + mg as u32 + mb as u32;
    assert!(multiply_luma < normal_luma,
        "multiply should produce darker result than normal; normal_luma={normal_luma} multiply_luma={multiply_luma}");
}

// ── Sticky positioning inside a scrollable div ────────────────────────────────

#[test]
fn layout_sticky_inside_scrollable_div() {
    use super::harness::find_box;
    // A sticky header inside a fixed-height overflow:scroll container.
    // The sticky element should NOT move past clip.y (the top of the div in screen space).
    // Previously the threshold was 0 (top of screen) instead of clip.y, so sticky never
    // kicked in for elements inside a div.
    //
    // We verify layout: the sticky-header exists and has a valid position inside the container.
    let doc = super::harness::parse_and_layout(r#"
        <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        .container { height: 200px; overflow-y: scroll; position: relative; width: 300px; }
        .sticky-hdr { position: sticky; top: 0; height: 30px; background: red; }
        .content    { height: 600px; }
        </style>
        <div class="container">
          <div class="sticky-hdr">Header</div>
          <div class="content"></div>
        </div>
    "#, 300.0);
    let hdr = find_box(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "sticky-hdr").unwrap_or(false)
    }).expect("sticky-hdr not found");
    let container = find_box(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "container").unwrap_or(false)
    }).expect("container not found");
    // Header must be inside the container vertically
    assert!(hdr.border_rect.y >= container.border_rect.y,
        "sticky-hdr should be at or below container top; hdr.y={} container.y={}",
        hdr.border_rect.y, container.border_rect.y);
    assert!((hdr.border_rect.h - 30.0).abs() < 1.0,
        "sticky-hdr height should be 30, got {}", hdr.border_rect.h);
}

// ── inline-block in flex: background must cover padding ──────────────────────

#[test]
fn layout_inline_block_in_flex_padding_covered() {
    use super::harness::find_box;
    // An inline-block button with horizontal padding inside a flex row nested in a two-column
    // layout (sidebar + content), mirroring graph.html structure.
    // border_rect.w must equal content_rect.w + left_padding + right_padding (24px total).
    // Bug: background (border_rect) was smaller than content+padding, clipping right padding,
    // caused by the outer flex over-shrinking the content column.
    let doc = super::harness::parse_and_layout(r#"
        <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        .main    { display: flex; }
        .sidebar { width: 170px; min-width: 170px; }
        .content { flex: 1; min-width: 0; }
        .btn-row { display: flex; flex-wrap: wrap; gap: 8px; }
        .btn     { display: inline-block; padding: 5px 12px; background: blue; }
        </style>
        <div class="main">
          <div class="sidebar">Sidebar</div>
          <div class="content">
            <div class="btn-row">
              <span class="btn">All Bar</span>
              <span class="btn">All Line</span>
            </div>
          </div>
        </div>
    "#, 800.0);
    let btn = find_box(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "btn").unwrap_or(false)
    }).expect("btn not found");
    let pad_total = 12.0 + 12.0; // padding-left + padding-right
    let expected_border_w = btn.content_rect.w + pad_total;
    assert!((btn.border_rect.w - expected_border_w).abs() < 1.5,
        "btn border_rect.w should be content_rect.w + 24 = {expected_border_w}, got {}; \
         content_rect.w={}", btn.border_rect.w, btn.content_rect.w);
    // content must be non-trivial (text was measured)
    assert!(btn.content_rect.w > 5.0,
        "btn content_rect.w should be > 5 (text width), got {}", btn.content_rect.w);
    // sidebar must keep its min-width (not be over-shrunk by flex)
    let sidebar = find_box(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "sidebar").unwrap_or(false)
    }).expect("sidebar not found");
    assert!(sidebar.border_rect.w >= 169.0,
        "sidebar must not shrink below min-width 170; got {}", sidebar.border_rect.w);
}

// ── float:right inside a block renders to the right ──────────────────────────

#[test]
fn layout_float_right_appears_on_right() {
    use super::harness::find_box;
    // A float:right element inside a fixed-width block should be positioned
    // at the right edge of the parent's content area.
    // Bug: float:right elements inside sb-item were not visible at all.
    let doc = super::harness::parse_and_layout(r#"
        <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        .item  { width: 170px; padding: 5px 8px; }
        .stat  { float: right; }
        </style>
        <div class="item">/home <span class="stat">4,231</span></div>
    "#, 200.0);
    let stat = find_box(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "stat").unwrap_or(false)
    }).expect("stat not found");
    let item = find_box(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "item").unwrap_or(false)
    }).expect("item not found");
    // The float must be to the right of the midpoint of the content area
    let content_mid = item.content_rect.x + item.content_rect.w / 2.0;
    assert!(stat.border_rect.x > content_mid,
        "float:right stat should be in right half; stat.x={} content_mid={} item.content={:?}",
        stat.border_rect.x, content_mid, item.content_rect);
    // Float right edge should be near the content right edge
    let stat_right  = stat.border_rect.x + stat.border_rect.w;
    let content_right = item.content_rect.x + item.content_rect.w;
    assert!((stat_right - content_right).abs() < 2.0,
        "float:right right edge should align with content right; stat_right={stat_right} content_right={content_right}");
}

#[test]
fn debug_graph_sidebar_and_button() {
    use super::harness::{find_box, find_all_boxes, parse_and_layout};
    let doc = parse_and_layout(r#"
        <style>
        * { margin: 0; padding: 0; box-sizing: border-box; font-size: 10pt; }
        body { background: #0d1117; }
        .main { display: flex; }
        .sidebar { background: #161b22; border-right: 1px solid #30363d;
                   padding: 14px; width: 170px; min-width: 170px; }
        .sidebar h3 { font-size: 9pt; margin: 0 0 8px 0; }
        .sb-item { padding: 5px 8px; margin-bottom: 2px; border-radius: 6px; font-size: 8pt; }
        .sb-item .sstat { float: right; }
        .content { flex: 1; min-width: 0; }
        .btn-row { display: flex; gap: 8px; padding: 0 16px 12px 16px; flex-wrap: wrap; }
        .btn { padding: 5px 12px; border-radius: 6px; font-size: 8pt;
               font-weight: 600; display: inline-block; background: blue; }
        </style>
        <div class="main">
          <div class="sidebar">
            <h3>Pages</h3>
            <div class="sb-item" id="sb-home">/home <span class="sstat">4,231</span></div>
            <div class="sb-item" id="sb-products">/products <span class="sstat">2,847</span></div>
          </div>
          <div class="content">
            <div class="btn-row">
              <span class="btn" id="btn-bar">All Bar</span>
              <span class="btn" id="btn-line">All Line</span>
            </div>
          </div>
        </div>
    "#, 1024.0);

    // Check sidebar width
    let sidebar = find_box(&doc.root, &|b| b.attributes.get("class").map(|c| c == "sidebar").unwrap_or(false)).unwrap();
    eprintln!("sidebar border_rect={:?}", sidebar.border_rect);

    // Check sstat float positions
    let sstats = find_all_boxes(&doc.root, &|b| b.attributes.get("class").map(|c| c == "sstat").unwrap_or(false));
    for s in &sstats {
        eprintln!("sstat border_rect={:?} margin_rect={:?}", s.border_rect, s.margin_rect);
    }

    // Check buttons
    let btns = find_all_boxes(&doc.root, &|b| matches!(b.attributes.get("class"), Some(c) if c.contains("btn")));
    for b in &btns {
        eprintln!("btn '{}' border_rect={:?} content_rect={:?}", b.attributes.get("id").unwrap_or(&String::new()), b.border_rect, b.content_rect);
    }
}

#[test]
fn debug_sidebar_box_sizing() {
    use super::harness::{find_box, parse_and_layout};
    use crate::types::BoxSizing;
    let doc = parse_and_layout(r#"
        <style>
        * { box-sizing: border-box; }
        .sidebar { padding: 14px; width: 170px; border-right: 1px solid red; }
        </style>
        <div style="display:flex;">
          <div class="sidebar">Sidebar</div>
          <div>Content</div>
        </div>
    "#, 800.0);
    let sidebar = find_box(&doc.root, &|b| {
        b.attributes.get("class").map(|c| c == "sidebar").unwrap_or(false)
    }).unwrap();
    eprintln!("sidebar box_sizing={:?} border_rect.w={} content_rect.w={}",
        sidebar.style.box_sizing, sidebar.border_rect.w, sidebar.content_rect.w);
    assert!(matches!(sidebar.style.box_sizing, BoxSizing::BorderBox),
        "sidebar should have box-sizing:border-box from * rule");
    assert!((sidebar.border_rect.w - 170.0).abs() < 1.0,
        "sidebar border_rect.w should be 170 (border-box), got {}", sidebar.border_rect.w);
}


