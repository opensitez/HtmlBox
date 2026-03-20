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
use std::io::Read;
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
            "--no-images" => { no_images = true; }
            "--no-cache"  => { no_cache  = true; }
            other if !other.starts_with("--") && url.is_empty() => { url = other.to_string(); }
            other => { eprintln!("Unknown argument: {other}"); }
        }
        i += 1;
    }

    if url.is_empty() {
        eprintln!("Usage: cargo run --example snapshot -- [--url] <url> [--width 1280] [--height 4000] [--out snapshot.png] [--scale 1] [--debug] [--no-images] [--no-cache] [--cache-dir snapshot_cache]");
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
    let (css_tx, css_rx) = mpsc::channel::<String>();
    let base      = url.clone();
    let css_tx2   = css_tx.clone();
    let no_cache2 = no_cache;
    let cache_dir2 = cache_dir.clone();

    let mut doc = parse_html_with_hooks(&html, &url, move |tag, attrs| {
        if tag == "link"
            && attrs.get("rel").map(|s| s == "stylesheet").unwrap_or(false)
        {
            if let Some(href) = attrs.get("href") {
                let abs       = resolve_url(&base, href);
                let sender    = css_tx2.clone();
                let cd        = cache_dir2.clone();
                std::thread::spawn(move || {
                    eprintln!("  CSS  {abs}");
                    let text = cached_fetch_text(&abs, no_cache2, &cd).unwrap_or_default();
                    let _ = sender.send(text);
                });
            }
        }
    });
    drop(css_tx);

    let css_sheets: Vec<String> = css_rx.iter().collect();
    let mut had_css = false;
    for sheet in &css_sheets {
        if !sheet.is_empty() {
            doc.stylesheet.parse_and_add(sheet);
            had_css = true;
        }
    }
    if had_css {
        apply_cascade_vp(&mut doc.root, &doc.stylesheet, None, 16.0, width, max_h, std::ptr::null(), false);
    }

    // ── Layout ────────────────────────────────────────────────────────────────

    eprintln!("Layout at {width}px × {max_h}px …");
    let mut renderer = Renderer::new();
    renderer.set_scale(scale);
    {
        let mut eng    = renderer.layout_engine();
        eng.viewport_h = max_h;
        eng.layout(&mut doc, width);
    }

    // ── Fetch images ──────────────────────────────────────────────────────────

    if !no_images {
        let mut img_srcs: Vec<String> = Vec::new();
        Document::walk_all(&doc.root, &mut |b| {
            if b.tag == "img" {
                if let Some(src) = b.attributes.get("src") {
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
                if let Ok(img) = image::load_from_memory(&bytes) {
                    let rgba      = img.to_rgba8();
                    let (iw, ih)  = (rgba.width(), rgba.height());
                    let raw       = rgba.into_raw();
                    let src2      = src.clone();
                    Document::walk_all_mut(&mut doc.root, &mut |b| {
                        if b.tag == "img" {
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
            eng.viewport_h = max_h;
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
}

// ─── Box tree dump ────────────────────────────────────────────────────────────

fn dump_box(depth: usize, node: &HtmlBox) {
    if matches!(node.style.display, Display::None) { return; }

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

    println!(
        "{}{}{}{} [{:?}] c=[{:.0},{:.0} {:.0}×{:.0}] m=[{:.0},{:.0} {:.0}×{:.0}]{}",
        indent, tag, id, cls,
        node.style.display,
        node.content_rect.x, node.content_rect.y, node.content_rect.w, node.content_rect.h,
        node.margin_rect.x,  node.margin_rect.y,  node.margin_rect.w,  node.margin_rect.h,
        text_preview,
    );

    for child in &node.children {
        dump_box(depth + 1, child);
    }
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
    String::from_utf8(bytes)
        .or_else(|e| {
            // Try latin-1 fallback for non-UTF-8 responses
            let bytes = e.into_bytes();
            Ok(bytes.iter().map(|&b| b as char).collect())
        })
}

// ─── Network helpers ──────────────────────────────────────────────────────────

fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(20))
        .call()
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    resp.into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    Ok(buf)
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
