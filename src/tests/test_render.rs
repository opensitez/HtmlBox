// Pixel-level render tests for blend modes and gradients.

use tiny_skia::Pixmap;
use crate::renderer::Renderer;
use super::harness::parse_and_layout;

fn render_html(html: &str, w: u32, h: u32) -> Pixmap {
    let mut doc = parse_and_layout(html, w as f32);
    let mut renderer = Renderer::new();
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
