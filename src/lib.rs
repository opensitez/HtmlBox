use std::io::Read as _;

pub mod types;
pub mod css;
pub mod html;
pub mod layout;
pub mod renderer;
pub mod platform;
pub mod dom;
pub mod markdown;
#[cfg(feature = "accessibility")]
pub mod accessibility;

#[cfg(test)]
pub mod tests;

pub use types::{Document, HtmlBox, ComputedStyle, Rect, Color, LivePoliteness, Announcement,
                KeyframeStop, EasingFn, AnimDirection, FillMode, ParsedAnimation, ParsedTransition,
                AnimState, TransitionState};
pub use markdown::{parse_markdown, serializer::serialize_markdown};
pub use html::{parse_html, parse_html_with_base, parse_html_with_hooks, parse_html_bytes, parse_html_bytes_with_base};
pub use layout::LayoutEngine;
pub use layout::hit_test::{HitResult, point_to_hit, offset_to_point, hit_test_box_at, hit_test_link, get_caret_x, get_offset_from_x};
pub use renderer::Renderer;
pub use dom::HtmlEventType;

/// High-level convenience: parse HTML, layout, ready to render.
pub fn load_html(html: &str, viewport_width: f32) -> Document {
    load_html_vp(html, viewport_width, 700.0)
}

/// Like `load_html` but with explicit viewport height (needed for `100vh` layouts).
pub fn load_html_vp(html: &str, viewport_width: f32, viewport_height: f32) -> Document {
    load_html_with_base(html, "", viewport_width, viewport_height)
}

/// Parse HTML with a base URL, fetch external CSS, layout, ready to render.
pub fn load_html_with_base(html: &str, base_url: &str, viewport_width: f32, viewport_height: f32) -> Document {
    load_html_with_registry(html, base_url, viewport_width, viewport_height, types::ComponentRegistry::default())
}

/// Parse HTML and layout with custom component registry.
/// External `<link rel="stylesheet">` tags trigger parallel CSS fetches during parsing
/// (like a browser), so network I/O overlaps with tokenisation.
pub fn load_html_with_registry(
    html: &str,
    base_url: &str,
    viewport_width: f32,
    viewport_height: f32,
    registry: types::ComponentRegistry,
) -> Document {
    use std::sync::{mpsc, atomic::{AtomicUsize, Ordering}, Arc};

    // Channel for CSS results — fetches start during parsing via the hook.
    let (css_tx, css_rx) = mpsc::channel::<(usize, String)>();
    let css_tx2  = css_tx.clone();
    let css_idx  = Arc::new(AtomicUsize::new(0));
    let css_idx2 = css_idx.clone();
    let base_owned = base_url.to_string();

    let t0 = std::time::Instant::now();
    let mut doc = parse_html_with_hooks(html, base_url, move |tag, attrs| {
        if tag == "link"
            && attrs.get("rel").map(|s| s == "stylesheet").unwrap_or(false)
        {
            if let Some(href) = attrs.get("href") {
                let abs    = resolve_css_url(&base_owned, href);
                eprintln!("  CSS fetch: {abs}");
                let sender = css_tx2.clone();
                let idx    = css_idx2.fetch_add(1, Ordering::SeqCst);
                std::thread::spawn(move || {
                    let t = std::time::Instant::now();
                    let text = fetch_text(&abs).unwrap_or_default();
                    eprintln!("  CSS done:  {} ({:.0}ms, {} bytes)", abs, t.elapsed().as_millis(), text.len());
                    let _ = sender.send((idx, text));
                });
            }
        }
    });
    eprintln!("Parse: {:.0}ms", t0.elapsed().as_millis());
    drop(css_tx); // close sender so rx.iter() terminates after all threads finish

    // Collect fetched stylesheets — wait up to 2s (CSS already started during parsing).
    let t1 = std::time::Instant::now();
    let expected_count = css_idx.load(std::sync::atomic::Ordering::SeqCst);
    let mut css_results: Vec<(usize, String)> = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while css_results.len() < expected_count {
        match css_rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())) {
            Ok(item) => css_results.push(item),
            Err(_) => break, // timeout or disconnected
        }
    }
    eprintln!("CSS wait: {:.0}ms ({}/{} sheets)", t1.elapsed().as_millis(), css_results.len(), expected_count);
    css_results.sort_by_key(|(idx, _)| *idx);
    for (_, sheet) in &css_results {
        if !sheet.is_empty() {
            doc.stylesheet.parse_and_add(sheet);
        }
    }

    // Start async image fetches (non-blocking — results arrive via poll_pending_images).
    start_async_image_fetches(&mut doc);

    // Re-run cascade with the real viewport so @media queries (min-width, max-width, etc.)
    // are evaluated against the actual window size rather than the default vw=0, vh=0.
    let t2 = std::time::Instant::now();
    doc.stylesheet.rebuild_index();
    eprintln!("  Cascade start ({} rules)...", doc.stylesheet.rules.len());
    css::apply_cascade_vp(&mut doc.root, &doc.stylesheet, None, 16.0, viewport_width, viewport_height, std::ptr::null(), false);
    eprintln!("  Cascade: {:.0}ms", t2.elapsed().as_millis());

    // Post-cascade: load background images (now that cascade has set background_image_url)
    html::load_background_images(&mut doc.root, &doc.base_url.clone());

    let t3 = std::time::Instant::now();
    let mut engine = LayoutEngine::new();
    engine.viewport_w = viewport_width;
    engine.viewport_h = viewport_height;
    engine.component_registry = registry;
    engine.layout(&mut doc, viewport_width);
    eprintln!("  Layout: {:.0}ms", t3.elapsed().as_millis());
    // Fire DOMContentLoaded — listeners registered before load_html can react.
    let evt = dom::HtmlEvent::new(dom::HtmlEventType::DOMContentLoaded);
    doc.events.dispatch(&doc.root, evt);
    doc
}

/// Walk the DOM tree, find all <img> nodes with remote `_resolved_src`,
/// fire off parallel fetch threads, store channel on Document for async polling.
fn start_async_image_fetches(doc: &mut types::Document) {
    let mut pending: Vec<(Vec<usize>, String)> = Vec::new();
    collect_remote_images(&doc.root, &mut Vec::new(), &mut pending);
    if pending.is_empty() { return; }

    let (tx, rx) = std::sync::mpsc::channel::<(Vec<usize>, Vec<u8>, u32, u32)>();
    let in_flight = doc.images_in_flight.clone();
    in_flight.store(pending.len(), std::sync::atomic::Ordering::SeqCst);

    for (path, url) in pending {
        let sender = tx.clone();
        let counter = in_flight.clone();
        std::thread::spawn(move || {
            let result = ureq::get(&url)
                .timeout(std::time::Duration::from_secs(10))
                .call().ok()
                .and_then(|r| {
                    let mut buf = Vec::new();
                    r.into_reader().read_to_end(&mut buf).ok()?;
                    Some(buf)
                })
                .and_then(|bytes| html::decode_image_bytes(&bytes));
            if let Some((data, w, h)) = result {
                let _ = sender.send((path, data, w, h));
            }
            counter.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        });
    }
    doc.pending_images = Some(rx);
}

fn collect_remote_images(
    node: &types::HtmlBox,
    path: &mut Vec<usize>,
    pending: &mut Vec<(Vec<usize>, String)>,
) {
    if node.tag == "img" && node.image_data.is_none() {
        if let Some(url) = node.attributes.get("_resolved_src") {
            if url.starts_with("http://") || url.starts_with("https://") {
                pending.push((path.clone(), url.clone()));
            }
        }
    }
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        collect_remote_images(child, path, pending);
        path.pop();
    }
}

fn resolve_css_url(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") { return href.to_string(); }
    if href.starts_with("//") {
        let scheme = if base.starts_with("https") { "https:" } else { "http:" };
        return format!("{scheme}{href}");
    }
    if let Some(p) = base.find("://") {
        let rest = &base[p + 3..];
        let origin_end = rest.find('/').map(|i| p + 3 + i).unwrap_or(base.len());
        let origin = &base[..origin_end];
        if href.starts_with('/') { return format!("{origin}{href}"); }
        let dir = if let Some(i) = base.rfind('/') {
            if &base[..i] == "https:" || &base[..i] == "http:" { base } else { &base[..i + 1] }
        } else { base };
        return format!("{dir}{href}");
    }
    href.to_string()
}

fn fetch_text(url: &str) -> Result<String, String> {
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    resp.into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    String::from_utf8(buf)
        .or_else(|e| {
            let bytes = e.into_bytes();
            Ok(bytes.iter().map(|&b| b as char).collect())
        })
}
