//! snapshot — headless URL renderer for debugging rhtmledit on real pages.
//!
//! Usage:
//!   cargo run --example snapshot -- [--url] <url> [OPTIONS]
//!
//! Options:
//!   --url <url>        URL or file path to render (or first positional arg)
//!   --width <px>       Viewport width in CSS pixels  (default: 1280)
//!   --height <px>      Max rendered height in CSS pixels (default: 4000)
//!   --out <file.png>   Output PNG path (default: snapshot.png)
//!   --scale <n>        Device pixel ratio for HiDPI output (default: 1)
//!   --debug            Dump the box tree to stdout after rendering
//!   --vars             Dump resolved CSS variables (from :root) to stdout
//!   --inspect <sel>    Inspect element(s) matching selector (e.g. "#id", ".class", "div.foo")
//!   --no-images        Skip image fetching (faster, for layout-only debugging)
//!   --no-cache         Always fetch from network, skip local cache
//!   --cache-dir <dir>  Cache directory (default: snapshot_cache)
//!
//! Examples:
//!   cargo run --example snapshot -- https://example.com
//!   cargo run --example snapshot -- file:///tmp/test.html --debug
//!   cargo run --example snapshot -- news.ycombinator.com --out hn.png --width 1440
//!   cargo run --example snapshot -- https://en.wikipedia.org/wiki/Main_Page --no-cache

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use std::path::PathBuf;
use std::sync::mpsc;

use tiny_skia::{Color, Pixmap};

use rhtmledit::{parse_html_with_hooks, Renderer, Document};
use rhtmledit::css::apply_cascade_vp;
use rhtmledit::types::{Display, HtmlBox};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut url           = String::new();
    let mut width: f32    = 1280.0;
    let mut max_h: f32    = 4000.0;
    let mut out           = String::from("snapshot.png");
    let mut scale: f32    = 1.0;
    let mut debug         = false;
    let mut show_vars     = false;
    let mut inspect: Vec<String> = Vec::new();
    let mut no_images     = false;
    let mut no_cache      = false;
    let mut cache_dir     = String::from("snapshot_cache");

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--url"       => { i += 1; if i < args.len() { url       = args[i].clone(); } }
            "--width"     => { i += 1; if i < args.len() { width     = args[i].parse().unwrap_or(width); } }
            "--height"    => { i += 1; if i < args.len() { max_h     = args[i].parse().unwrap_or(max_h); } }
            "--out"       => { i += 1; if i < args.len() { out       = args[i].clone(); } }
            "--scale"     => { i += 1; if i < args.len() { scale     = args[i].parse().unwrap_or(scale); } }
            "--cache-dir" => { i += 1; if i < args.len() { cache_dir = args[i].clone(); } }
            "--debug"     => { debug     = true; }
            "--vars"      => { show_vars = true; }
            "--inspect"   => { i += 1; if i < args.len() { inspect.push(args[i].clone()); } }
            "--no-images" => { no_images = true; }
            "--no-cache"  => { no_cache  = true; }
            other if !other.starts_with("--") && url.is_empty() => { url = other.to_string(); }
            other => { eprintln!("Unknown argument: {other}"); }
        }
        i += 1;
    }

    if url.is_empty() {
        eprintln!("Usage: cargo run --example snapshot -- [--url] <url> [--width 1280] [--height 4000] [--out snapshot.png] [--scale 1] [--debug] [--vars] [--no-images] [--no-cache] [--cache-dir snapshot_cache]");
        std::process::exit(1);
    }

    let url = normalize_url(url);

    // ── Fetch HTML ────────────────────────────────────────────────────────────

    eprintln!("Fetching {url} …");
    let html = if let Some(path) = url.strip_prefix("file://") {
        std::fs::read_to_string(path)
            .unwrap_or_else(|e| format!("<h2>File error</h2><p>{e}</p>"))
    } else {
        match cached_fetch_text(&url, no_cache, &cache_dir) {
            Ok(s) => s,
            Err(e) => format!("<h2>Fetch error</h2><p>{e}</p>"),
        }
    };

    // ── Parse HTML + fetch external CSS in parallel ───────────────────────────

    eprintln!("Parsing …");
    // Collect CSS hrefs in declaration order during parse, fetch in parallel,
    // then apply in declaration order so later sheets correctly override earlier ones.
    let (css_tx, css_rx) = mpsc::channel::<(usize, String, String)>();
    let base        = url.clone();
    let css_tx2     = css_tx.clone();
    let no_cache2   = no_cache;
    let cache_dir2  = cache_dir.clone();
    let css_idx     = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let css_idx2    = css_idx.clone();

    let mut doc = parse_html_with_hooks(&html, &url, move |tag, attrs| {
        if tag == "link"
            && attrs.get("rel").map(|s| s == "stylesheet").unwrap_or(false)
        {
            if let Some(href) = attrs.get("href") {
                let abs    = resolve_url(&base, href);
                let sender = css_tx2.clone();
                let cd     = cache_dir2.clone();
                let idx    = css_idx2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                std::thread::spawn(move || {
                    eprintln!("  CSS  {abs}");
                    let text = cached_fetch_text(&abs, no_cache2, &cd).unwrap_or_default();
                    let _ = sender.send((idx, abs, text));
                });
            }
        }
    });
    drop(css_tx);

    // Collect results and sort by original declaration index before applying.
    let mut css_results: Vec<(usize, String, String)> = css_rx.iter().collect();
    css_results.sort_by_key(|(idx, _, _)| *idx);

    let mut had_css = false;
    for (_, css_url, sheet) in &css_results {
        if !sheet.is_empty() {
            doc.stylesheet.parse_and_add_with_base(sheet, css_url);
            had_css = true;
        }
    }
    if had_css {
        doc.stylesheet.resolve_variables_for_viewport(width, max_h);
        if !inspect.is_empty() { doc.stylesheet.inspect_mode = true; }
        apply_cascade_vp(&mut doc.root, &doc.stylesheet, None, 16.0, width, max_h, 0, false);
    }

    // ── Dump CSS variables ──────────────────────────────────────────────────
    if show_vars {
        println!("\n=== CSS VARIABLES ({} total) ===", doc.stylesheet.variables.len());
        let mut vars: Vec<_> = doc.stylesheet.variables.iter().collect();
        vars.sort_by_key(|(k, _)| k.clone());
        for (k, v) in &vars {
            println!("  {}: {}", k, v);
        }
    }

    // ── Layout ────────────────────────────────────────────────────────────────

    eprintln!("Layout at {width}px × {max_h}px …");
    let mut renderer = Renderer::new();
    renderer.set_scale(scale);
    {
        let mut eng    = renderer.layout_engine();
        eng.viewport_h = 900.0;
        eng.layout(&mut doc, width);
    }

    // Post-layout: load background images (layout may re-run cascade with viewport dims)
    rhtmledit::html::load_background_images(&mut doc.root, &url);

    // ── Fetch images ──────────────────────────────────────────────────────────

    if !no_images {
        let mut img_srcs: Vec<String> = Vec::new();
        Document::walk_all(&doc.root, &mut |b| {
            let is_img = b.tag == "img"
                || (b.tag == "input" && b.attributes.get("type").map(|s| s.as_str()) == Some("image"));
            if is_img {
                if let Some(src) = b.attributes.get("src") {
                    if src.starts_with("__svg_") { return; }
                    let abs = resolve_url(&url, src);
                    if !abs.is_empty() && !img_srcs.contains(&abs) {
                        img_srcs.push(abs);
                    }
                }
            }
        });

        let mut any_image = false;
        for src in &img_srcs {
            eprintln!("  IMG  {src}");
            let bytes_result = if src.starts_with("data:") {
                decode_data_url(src)
            } else {
                cached_fetch_bytes(src, no_cache, &cache_dir)
            };
            if let Ok(bytes) = bytes_result {
                // Try raster image first, then SVG via resvg
                let decoded = image::load_from_memory(&bytes)
                    .map(|img| {
                        let rgba = img.to_rgba8();
                        let (w, h) = (rgba.width(), rgba.height());
                        (rgba.into_raw(), w, h)
                    })
                    .ok()
                    .or_else(|| {
                        // Try as SVG at intrinsic dimensions
                        let svg_str = std::str::from_utf8(&bytes).ok()?;
                        rhtmledit::html::rasterize_svg_intrinsic(svg_str)
                    });
                if let Some((raw, iw, ih)) = decoded {
                    let src2      = src.clone();
                    Document::walk_all_mut(&mut doc.root, &mut |b| {
                        let is_img = b.tag == "img"
                            || (b.tag == "input" && b.attributes.get("type").map(|s| s.as_str()) == Some("image"));
                        if is_img {
                            if let Some(s) = b.attributes.get("src") {
                                if resolve_url(&url, s) == src2 {
                                    b.image_data   = Some(raw.clone());
                                    b.image_width  = iw;
                                    b.image_height = ih;
                                    b.layout_dirty = true;
                                }
                            }
                        }
                    });
                    any_image = true;
                }
            }
        }
        if any_image {
            let mut eng    = renderer.layout_engine();
            eng.viewport_h = 900.0;
            eng.layout(&mut doc, width);
        }
    }

    // ── Render to pixmap ──────────────────────────────────────────────────────

    let doc_h    = (doc.root.margin_rect.h.ceil() as u32).max(1);
    let render_h = doc_h.min(max_h as u32).max(1);
    let phys_w   = (width  * scale) as u32;
    let phys_h   = (render_h as f32 * scale) as u32;

    eprintln!("Rendering {phys_w}×{phys_h}px (doc height: {doc_h}px, scale: {scale}) …");

    let mut pixmap = Pixmap::new(phys_w.max(1), phys_h.max(1))
        .expect("Failed to allocate pixmap");
    pixmap.fill(Color::WHITE);
    renderer.render(&mut doc, &mut pixmap, scale);

    pixmap.save_png(&out).expect("Failed to save PNG");
    eprintln!("Saved → {out}");

    // ── Debug box tree ────────────────────────────────────────────────────────

    if debug {
        println!("\n=== BOX TREE ===");
        dump_box(0, &doc.root);
    }

    // ── Inspect elements ─────────────────────────────────────────────────────
    for query in &inspect {
        println!("\n=== INSPECT: {} ===", query);
        let mut found = false;
        inspect_walk(&doc.root, query, &mut found);
        if !found { println!("  (no matching element found)"); }
    }
}

// ─── Box tree dump ────────────────────────────────────────────────────────────

fn dump_box(depth: usize, node: &HtmlBox) {
    // Show display:none nodes that have id/class for debugging
    if matches!(node.style.display, Display::None) {
        let has_id = node.attributes.get("id").is_some();
        let has_class = node.attributes.get("class").map(|c| !c.is_empty()).unwrap_or(false);
        if has_id || has_class || matches!(node.tag.as_str(), "html" | "body" | "head") {
            let indent = "  ".repeat(depth);
            let id = node.attributes.get("id").map(|v| format!("#{v}")).unwrap_or_default();
            let cls = node.attributes.get("class")
                .map(|v| format!(".{}", v.split_whitespace().take(3).collect::<Vec<_>>().join(".")))
                .unwrap_or_default();
            let n_children = node.children.len();
            println!("{}{}{}{} [HIDDEN display:none] ({} children)", indent, node.tag, id, cls, n_children);
            // For critical elements, recurse 1 level to show what's inside
            if matches!(node.tag.as_str(), "html" | "body") {
                for child in &node.children {
                    dump_box(depth + 1, child);
                }
            }
        }
        return;
    }

    let indent = "  ".repeat(depth);
    let tag    = if node.tag.is_empty() { "(box)" } else { &node.tag };

    let id  = node.attributes.get("id")
        .map(|v| format!("#{v}"))
        .unwrap_or_default();
    let cls = node.attributes.get("class")
        .map(|v| format!(".{}", v.split_whitespace().collect::<Vec<_>>().join(".")))
        .unwrap_or_default();

    let text_preview = if node.tag == "#text" && !node.text.is_empty() {
        let s: String = node.text.chars().take(40).collect();
        format!(" {:?}", s.trim())
    } else {
        String::new()
    };

    let flex_dir_str = if matches!(node.style.display, rhtmledit::types::Display::Flex | rhtmledit::types::Display::InlineFlex) {
        match node.style.flex_direction {
            rhtmledit::types::FlexDirection::Row => " flex:row",
            rhtmledit::types::FlexDirection::RowReverse => " flex:row-rev",
            rhtmledit::types::FlexDirection::Column => " flex:col",
            rhtmledit::types::FlexDirection::ColumnReverse => " flex:col-rev",
        }
    } else { "" };
    let float_str = match node.style.float {
        rhtmledit::types::Float::Left  => " float:left",
        rhtmledit::types::Float::Right => " float:right",
        _ => "",
    };
    let pos_str = match node.style.position {
        rhtmledit::types::Position::Relative => " pos:rel",
        rhtmledit::types::Position::Absolute => " pos:abs",
        rhtmledit::types::Position::Fixed    => " pos:fixed",
        rhtmledit::types::Position::Sticky   => " pos:sticky",
        _ => "",
    };
    let font_sz = match node.style.font_size {
        rhtmledit::types::CssLength::Px(v) => format!(" fs:{v:.1}px"),
        _ => String::new(),
    };

    // Background color (only show if not transparent)
    let bg_str = {
        let bg = node.style.background_color;
        if bg.a > 0 {
            format!(" bg:#{:02x}{:02x}{:02x}{}", bg.r, bg.g, bg.b, if bg.a < 255 { format!("/{}", bg.a) } else { String::new() })
        } else { String::new() }
    };
    // Color (only for text nodes or elements with explicit color)
    let color_str = if node.tag == "#text" {
        let c = node.style.color;
        format!(" color:#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    } else { String::new() };
    // Overflow
    let overflow_str = match (node.style.overflow_x, node.style.overflow_y) {
        (rhtmledit::types::Overflow::Hidden, rhtmledit::types::Overflow::Hidden) => " overflow:hidden",
        (rhtmledit::types::Overflow::Hidden, _) => " overflow-x:hidden",
        (_, rhtmledit::types::Overflow::Hidden) => " overflow-y:hidden",
        (rhtmledit::types::Overflow::Scroll, _) | (_, rhtmledit::types::Overflow::Scroll) => " overflow:scroll",
        _ => "",
    };
    // Image data
    let img_str = if node.image_data.is_some() {
        format!(" img:{}×{}", node.image_width, node.image_height)
    } else { String::new() };
    // Padding rect (only show if different from content rect)
    let pad_str = if node.padding_rect.w != node.content_rect.w || node.padding_rect.h != node.content_rect.h {
        format!(" p=[{:.0},{:.0} {:.0}×{:.0}]",
            node.padding_rect.x, node.padding_rect.y, node.padding_rect.w, node.padding_rect.h)
    } else { String::new() };

    println!(
        "{}{}{}{} [{:?}{}{}{}] c=[{:.0},{:.0} {:.0}×{:.0}]{} m=[{:.0},{:.0} {:.0}×{:.0}]{}{}{}{}{}{}",
        indent, tag, id, cls,
        node.style.display, flex_dir_str, float_str, pos_str,
        node.content_rect.x, node.content_rect.y, node.content_rect.w, node.content_rect.h,
        pad_str,
        node.margin_rect.x,  node.margin_rect.y,  node.margin_rect.w,  node.margin_rect.h,
        font_sz, bg_str, color_str, overflow_str, img_str, text_preview,
    );

    // Dump line cache if present
    if !node.line_cache.is_empty() {
        for (li, line) in node.line_cache.iter().enumerate() {
            let segs: String = line.visual_segments.iter()
                .map(|s| format!("(x{:.0} w{:.0})", s.x, s.width))
                .collect::<Vec<_>>().join(" ");
            println!("{}  line[{}]: x={:.0} y={:.0} w={:.0} h={:.0} segs=[{}] chars={}",
                indent, li, line.x, line.y, line.width, line.height, segs, line.char_x.len());
        }
    }
    // Dump shadow root children if present
    if let Some(ref sr) = node.shadow_root {
        let indent = "  ".repeat(depth + 1);
        println!("{}#shadow-root ({:?}, {} children)", indent, sr.mode, sr.children.len());
        for child in &sr.children {
            dump_box(depth + 2, child);
        }
    }
    for child in &node.children {
        dump_box(depth + 1, child);
    }
}

// ─── Element inspector ───────────────────────────────────────────────────────

/// Match an element against an inspect query.
/// Supports: "#id", ".class", "tag", "tag.class", "tag#id", ".class1.class2"
fn matches_query(node: &HtmlBox, query: &str) -> bool {
    if node.tag == "#text" { return false; }
    let query = query.trim();
    // Split query into parts: tag, #id, .class segments
    let mut tag_q = "";
    let mut id_q  = "";
    let mut classes_q: Vec<&str> = Vec::new();
    let mut rest = query;
    // Leading tag (before any # or .)
    if !rest.starts_with('#') && !rest.starts_with('.') {
        let end = rest.find(|c: char| c == '#' || c == '.').unwrap_or(rest.len());
        tag_q = &rest[..end];
        rest = &rest[end..];
    }
    while !rest.is_empty() {
        if rest.starts_with('#') {
            rest = &rest[1..];
            let end = rest.find(|c: char| c == '#' || c == '.').unwrap_or(rest.len());
            id_q = &rest[..end];
            rest = &rest[end..];
        } else if rest.starts_with('.') {
            rest = &rest[1..];
            let end = rest.find(|c: char| c == '#' || c == '.').unwrap_or(rest.len());
            classes_q.push(&rest[..end]);
            rest = &rest[end..];
        } else {
            break;
        }
    }
    if !tag_q.is_empty() && !node.tag.eq_ignore_ascii_case(tag_q) { return false; }
    if !id_q.is_empty() {
        if node.attributes.get("id").map(|s| s.as_str()) != Some(id_q) { return false; }
    }
    if !classes_q.is_empty() {
        let cls = node.attributes.get("class").map(|s| s.as_str()).unwrap_or("");
        let elem_classes: Vec<&str> = cls.split_whitespace().collect();
        for c in &classes_q {
            if !elem_classes.contains(c) { return false; }
        }
    }
    true
}

fn inspect_walk(node: &HtmlBox, query: &str, found: &mut bool) {
    if matches_query(node, query) {
        *found = true;
        inspect_print(node);
    }
    for child in &node.children {
        inspect_walk(child, query, found);
    }
}

fn inspect_print(node: &HtmlBox) {
    let s = &node.style;
    let id  = node.attributes.get("id").map(|v| format!("#{v}")).unwrap_or_default();
    let cls = node.attributes.get("class").map(|v| format!(".{}", v.split_whitespace().collect::<Vec<_>>().join("."))).unwrap_or_default();
    println!("  <{}{}{}> ", node.tag, id, cls);
    println!("  ── Box Model ──");
    println!("    content:  ({:.1}, {:.1}) {:.1} × {:.1}", node.content_rect.x, node.content_rect.y, node.content_rect.w, node.content_rect.h);
    println!("    padding:  ({:.1}, {:.1}) {:.1} × {:.1}", node.padding_rect.x, node.padding_rect.y, node.padding_rect.w, node.padding_rect.h);
    println!("    margin:   ({:.1}, {:.1}) {:.1} × {:.1}", node.margin_rect.x, node.margin_rect.y, node.margin_rect.w, node.margin_rect.h);
    println!("  ── Computed Style ──");
    println!("    display:        {:?}", s.display);
    println!("    position:       {:?}", s.position);
    println!("    float:          {:?}", s.float);
    println!("    overflow:       {:?} / {:?}", s.overflow_x, s.overflow_y);
    println!("    box-sizing:     {:?}", s.box_sizing);
    println!("    width:          {:?}", s.width);
    println!("    height:         {:?}", s.height);
    println!("    min-width:      {:?}", s.min_width);
    println!("    max-width:      {:?}", s.max_width);
    println!("    min-height:     {:?}", s.min_height);
    println!("    max-height:     {:?}", s.max_height);
    println!("    flex-direction: {:?}", s.flex_direction);
    println!("    flex-wrap:      {:?}", s.flex_wrap);
    println!("    flex-grow:      {}", s.flex_grow);
    println!("    flex-shrink:    {}", s.flex_shrink);
    println!("    flex-basis:     {:?}", s.flex_basis);
    println!("    align-items:    {:?}", s.align_items);
    println!("    align-self:     {:?}", s.align_self);
    println!("    justify-content:{:?}", s.justify_content);
    println!("    vertical-align: {:?}", s.vertical_align);
    let font_px = s.font_size_px(16.0, 16.0);
    println!("    font-size:      {:.1}px", font_px);
    println!("    color:          #{:02x}{:02x}{:02x}", s.color.r, s.color.g, s.color.b);
    let bg = s.background_color;
    if bg.a > 0 {
        println!("    background:     #{:02x}{:02x}{:02x}/{}", bg.r, bg.g, bg.b, bg.a);
    } else {
        println!("    background:     transparent");
    }
    // Margins/padding resolved
    println!("    margin:         {:.1} {:.1} {:.1} {:.1} (T R B L)",
        node.resolved_margin_top, node.resolved_margin_right, node.resolved_margin_bottom, node.resolved_margin_left);
    println!("    padding:        {:.1} {:.1} {:.1} {:.1} (T R B L)",
        node.resolved_pad_top, node.resolved_pad_right, node.resolved_pad_bottom, node.resolved_pad_left);
    println!("    border-width:   {:.1} {:.1} {:.1} {:.1} (T R B L)",
        node.resolved_border_top, node.resolved_border_right, node.resolved_border_bottom, node.resolved_border_left);
    if node.image_data.is_some() {
        println!("    image:          {}×{}", node.image_width, node.image_height);
    }
    // Matched CSS rules (when inspect mode is on)
    if !node.matched_rules.is_empty() {
        println!("  ── Matched Rules ({}) ──", node.matched_rules.len());
        for rule in &node.matched_rules {
            let src = if rule.source == "ua" { " (user-agent)" } else if rule.source.is_empty() { "" } else { " (media)" };
            println!("    {} [sp:{}]{}", rule.selector, rule.specificity, src);
            for (prop, val) in &rule.declarations {
                if prop.starts_with("--") { continue; } // skip custom properties
                println!("      {}: {}", prop, val);
            }
        }
    }
    // Children summary
    let n_children = node.children.iter().filter(|c| c.tag != "#text").count();
    if n_children > 0 {
        println!("  ── Children ({}) ──", n_children);
        for child in &node.children {
            if child.tag == "#text" { continue; }
            let cid  = child.attributes.get("id").map(|v| format!("#{v}")).unwrap_or_default();
            let ccls = child.attributes.get("class")
                .map(|v| format!(".{}", v.split_whitespace().take(3).collect::<Vec<_>>().join(".")))
                .unwrap_or_default();
            println!("    <{}{}{}> {:?} ({:.0}×{:.0}) @ ({:.0},{:.0})",
                child.tag, cid, ccls, child.style.display,
                child.content_rect.w, child.content_rect.h,
                child.content_rect.x, child.content_rect.y);
        }
    }
    println!();
}

// ─── Disk cache ───────────────────────────────────────────────────────────────

fn url_cache_path(url: &str, cache_dir: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let hash = hasher.finish();
    // Append a short sanitized suffix for readability
    let suffix: String = url.chars()
        .filter(|c| c.is_alphanumeric() || *c == '.')
        .take(40)
        .collect();
    PathBuf::from(cache_dir).join(format!("{hash:016x}_{suffix}"))
}

fn cached_fetch_bytes(url: &str, no_cache: bool, cache_dir: &str) -> Result<Vec<u8>, String> {
    let path = url_cache_path(url, cache_dir);
    if !no_cache {
        if let Ok(data) = std::fs::read(&path) {
            eprintln!("    [cache] {url}");
            return Ok(data);
        }
    }
    let data = fetch_bytes(url)?;
    let _ = std::fs::create_dir_all(cache_dir);
    let _ = std::fs::write(&path, &data);
    Ok(data)
}

fn cached_fetch_text(url: &str, no_cache: bool, cache_dir: &str) -> Result<String, String> {
    let bytes = cached_fetch_bytes(url, no_cache, cache_dir)?;
    Ok(String::from_utf8(bytes.clone()).unwrap_or_else(|_| {
        let (cow, _, _) = encoding_rs::WINDOWS_1252.decode(&bytes);
        cow.into_owned()
    }))
}

// ─── Network helpers ──────────────────────────────────────────────────────────

fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let do_fetch = |client: &reqwest::blocking::Client| -> Result<Vec<u8>, String> {
        let resp = client.get(url).send().map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        Ok(resp.bytes().map_err(|e| e.to_string())?.to_vec())
    };
    match do_fetch(&rhtmledit::http_client()) {
        Ok(b) if !b.is_empty() => Ok(b),
        _ => do_fetch(&rhtmledit::http_client_lenient()),
    }
}

fn decode_data_url(data_url: &str) -> Result<Vec<u8>, String> {
    // data:[<mediatype>][;base64],<data>
    let rest = data_url.strip_prefix("data:").ok_or("not a data URL")?;
    let comma = rest.find(',').ok_or("malformed data URL")?;
    let meta  = &rest[..comma];
    let data  = &rest[comma + 1..];
    if meta.ends_with(";base64") {
        // Simple base64 decode without external crate
        base64_decode(data)
    } else {
        Ok(data.as_bytes().to_vec())
    }
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let table: [u8; 128] = {
        let mut t = [255u8; 128];
        for (i, &c) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
            .iter()
            .enumerate()
        {
            t[c as usize] = i as u8;
        }
        t
    };
    let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=' && b < 128 && table[b as usize] != 255).collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let v: Vec<u8> = chunk.iter().map(|&b| table[b as usize]).collect();
        let n = v.len();
        if n >= 2 { out.push((v[0] << 2) | (v[1] >> 4)); }
        if n >= 3 { out.push((v[1] << 4) | (v[2] >> 2)); }
        if n >= 4 { out.push((v[2] << 6) | v[3]); }
    }
    Ok(out)
}

// ─── URL helpers ──────────────────────────────────────────────────────────────

fn normalize_url(s: String) -> String {
    let s = s.trim().to_string();
    if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("file://") {
        return s;
    }
    // Looks like a local path?
    if s.starts_with('/') || s.starts_with('.') {
        return format!("file://{s}");
    }
    format!("https://{s}")
}

fn resolve_url(base: &str, href: &str) -> String {
    if href.is_empty()                           { return base.to_string(); }
    if href.starts_with("data:")                 { return href.to_string(); }
    if href.starts_with("http://") || href.starts_with("https://") { return href.to_string(); }
    if href.starts_with("//") {
        let scheme = if base.starts_with("https") { "https:" } else { "http:" };
        return format!("{scheme}{href}");
    }
    let origin = if let Some(p) = base.find("://") {
        let rest  = &base[p + 3..];
        let slash = rest.find('/').map(|i| p + 3 + i).unwrap_or(base.len());
        &base[..slash]
    } else { "" };
    if href.starts_with('/') {
        return format!("{origin}{href}");
    }
    let dir = if let Some(i) = base.rfind('/') {
        if &base[..i] == "https:" || &base[..i] == "http:" { base } else { &base[..i + 1] }
    } else { base };
    format!("{dir}{href}")
}
